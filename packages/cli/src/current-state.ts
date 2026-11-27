/**
 * The explicit current-state cleanup plan.
 *
 * The engine's `plan` command is delta-shaped. Instead of adding a protocol
 * command, seed a synthetic internal delta from the current snapshot, ask the
 * engine for the plan, then augment the result so every eligible whole file is
 * in exactly one batch. The synthetic delta never reaches the user.
 */
import { engineConfig, type ShrikeConfig } from "./config.ts";
import { requestEngine } from "./engine.ts";
import { headCommit, isDirty } from "./git.ts";
import { analyze, attachBuildEvidence } from "./session.ts";
import { isEligibleFile } from "./eligibility.ts";
import { groupDirectory } from "./report/groups.ts";
import { intervalLength, mergeIntervals, type Interval } from "./report/metrics.ts";
import type { ParsedArgs } from "./args.ts";
import type { BuildEvidenceInfo, Plan, Snapshot, SymbolRecord } from "./cli-types.ts";

const DELTA_KINDS = new Set([
  "export",
  "function",
  "class",
  "variable",
  "type",
  "interface",
  "enum",
  "namespace",
  "default",
  "re-export",
  "file",
]);

type DeltaItem = {
  symbol: string;
  kind: string;
  file: string;
  lastWitness: string[];
  noWitnessReason: string;
  trigger: {
    file: string;
    kind: string;
    change: string;
    certainty: string;
  };
  confidence: string;
  estimatedLines: number;
};

/** The synthetic delta mirrors `schemas/delta.schema.json`, not the narrow TS alias. */
export interface CurrentDelta {
  schemaVersion: "1.0";
  base: { ref: string | null; commit: string };
  head: { commit: string; dirty: boolean; graphHash: string };
  newlyDead: DeltaItem[];
  newlyDeadFiles: string[];
  cascade: { symbols: number; files: number; lines: number };
  batches: Plan["batches"];
  incomparable: null;
  diagnostics: NonNullable<Snapshot["diagnostics"]>;
}

function deltaKind(symbol: SymbolRecord): string {
  if (symbol.kind === "module") return "file";
  return DELTA_KINDS.has(symbol.kind) ? symbol.kind : "export";
}

function triggerFor(symbol: SymbolRecord): DeltaItem["trigger"] {
  return {
    file: symbol.file,
    kind: "indirect",
    change: "dead at head with no witness from any entry",
    certainty: "certain",
  };
}

export function buildCurrentDelta(
  snapshot: Snapshot,
  head: string,
  dirty: boolean,
): CurrentDelta {
  const byKey = new Map(snapshot.symbols.map((symbol) => [symbol.key, symbol]));
  const eligible = new Map(
    snapshot.files.filter((file) => isEligibleFile(file)).map((file) => [file.path, file]),
  );
  const newlyDead: DeltaItem[] = [];
  const representedFiles = new Set<string>();
  for (const key of [...snapshot.candidates].sort()) {
    const symbol = byKey.get(key);
    if (!symbol || !eligible.has(symbol.file)) continue;
    representedFiles.add(symbol.file);
    newlyDead.push({
      symbol: symbol.key,
      kind: deltaKind(symbol),
      file: symbol.file,
      lastWitness: [],
      noWitnessReason: "unknown-edge",
      trigger: triggerFor(symbol),
      confidence: "certain",
      estimatedLines: symbol.lines,
    });
  }
  const unusedFiles = [...new Set(snapshot.unusedFiles)]
    .filter((path) => eligible.has(path))
    .sort();
  for (const path of unusedFiles) {
    if (representedFiles.has(path)) continue;
    const file = eligible.get(path);
    if (!file) continue;
    representedFiles.add(path);
    newlyDead.push({
      symbol: `${path}#__module__`,
      kind: "file",
      file: path,
      lastWitness: [],
      noWitnessReason: "unknown-edge",
      trigger: {
        file: path,
        kind: "indirect",
        change: "dead at head with no witness from any entry",
        certainty: "certain",
      },
      confidence: "certain",
      estimatedLines: file.lines,
    });
  }
  const files = new Set(newlyDead.map((item) => item.file));
  return {
    schemaVersion: "1.0",
    base: { ref: null, commit: head },
    head: { commit: head, dirty, graphHash: snapshot.graphHash },
    newlyDead,
    newlyDeadFiles: unusedFiles,
    cascade: {
      symbols: newlyDead.length,
      files: files.size,
      lines: newlyDead.reduce((total, item) => total + item.estimatedLines, 0),
    },
    batches: [],
    incomparable: null,
    diagnostics: snapshot.diagnostics ?? [],
  };
}

function nextBatchId(batches: readonly Plan["batches"][number][]): string {
  let highest = 0;
  for (const batch of batches) {
    const match = /^batch_(\d+)$/.exec(batch.id);
    if (match) highest = Math.max(highest, Number.parseInt(match[1] ?? "0", 10));
  }
  const used = new Set(batches.map((batch) => batch.id));
  let sequence = highest + 1;
  while (used.has(`batch_${sequence}`)) sequence += 1;
  return `batch_${sequence}`;
}

function directoryOf(path: string): string {
  return groupDirectory(path);
}

export function recountCurrentPlan(plan: Plan, snapshot: Snapshot): Plan {
  const result = structuredClone(plan);
  const files = new Map(snapshot.files.map((file) => [file.path, file]));
  const symbols = new Map(snapshot.symbols.map((symbol) => [symbol.key, symbol]));
  const unused = new Set(
    snapshot.unusedFiles.filter((path) => {
      const file = files.get(path);
      return file !== undefined && isEligibleFile(file);
    }),
  );
  const assigned = new Set<string>();
  for (const batch of result.batches) {
    for (const path of [...batch.files]) {
      if (!unused.has(path)) continue;
      const key = `${path}#__module__`;
      if (!batch.symbols.includes(key)) batch.symbols.push(key);
      assigned.add(path);
    }
    for (const key of batch.symbols) {
      if (!key.endsWith("#__module__")) continue;
      const path = key.slice(0, -"#__module__".length);
      if (!unused.has(path)) continue;
      if (!batch.files.includes(path)) batch.files.push(path);
      assigned.add(path);
    }
  }
  for (const path of [...unused].sort()) {
    if (assigned.has(path)) continue;
    const directory = directoryOf(path);
    let target: Plan["batches"][number] | undefined;
    let targetCount = 0;
    for (const batch of result.batches) {
      const count = batch.files.filter((file) => directoryOf(file) === directory).length;
      if (count > targetCount || (count === targetCount && count > 0 && target && batch.id < target.id)) {
        target = batch;
        targetCount = count;
      }
    }
    if (!target) {
      const id = nextBatchId(result.batches);
      target = {
        id,
        symbols: [],
        files: [],
        lines: 0,
        patch: `.shrike/patches/${id}.patch`,
      };
      result.batches.push(target);
    }
    target.symbols.push(`${path}#__module__`);
    target.files.push(path);
    assigned.add(path);
  }
  for (const batch of result.batches) {
    const batchFiles = new Set<string>(batch.files);
    for (const key of batch.symbols) {
      const symbol = symbols.get(key);
      if (symbol) batchFiles.add(symbol.file);
    }
    batch.files = [...batchFiles].sort();
    batch.symbols = [...new Set(batch.symbols)].sort();
    let lines = 0;
    for (const file of batch.files) {
      const record = files.get(file);
      if (!record) continue;
      if (unused.has(file)) {
        lines += record.lines;
        continue;
      }
      const intervals: Interval[] = [];
      for (const key of batch.symbols) {
        const symbol = symbols.get(key);
        if (!symbol || symbol.file !== file || key.endsWith("#__module__")) continue;
        const start = Math.max(1, symbol.span.line);
        const end = Math.min(record.lines, start + Math.max(0, symbol.lines - 1));
        if (end >= start) intervals.push({ start, end });
      }
      lines += intervalLength(mergeIntervals(intervals));
    }
    batch.lines = lines;
  }
  result.totals = {
    symbols: result.batches.reduce((total, batch) => total + batch.symbols.length, 0),
    files: result.batches.reduce((total, batch) => total + batch.files.length, 0),
    lines: result.batches.reduce((total, batch) => total + batch.lines, 0),
  };
  return result;
}

export async function buildCurrentPlan(
  root: string,
  config: ShrikeConfig,
  args?: ParsedArgs,
): Promise<{
  plan: Plan;
  snapshot: Snapshot;
  head: string;
  dirty: boolean;
  build: BuildEvidenceInfo | null;
  delta: CurrentDelta;
}> {
  const snapshot = await analyze(root, config);
  const head = headCommit(root);
  const dirty = isDirty(root);
  const build = args ? await attachBuildEvidence(root, snapshot, args, config) : null;
  const delta = buildCurrentDelta(snapshot, head, dirty);
  const plan = await requestEngine<Plan>("plan", {
    snapshot,
    delta,
    config: engineConfig(config),
  });
  return { plan: recountCurrentPlan(plan, snapshot), snapshot, head, dirty, build, delta };
}
