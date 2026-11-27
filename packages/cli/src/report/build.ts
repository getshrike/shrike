/**
 * Build a `CleanupReport` from engine facts. The builder groups, counts, and
 * explains; it never decides that code is dead.
 */
import type { DeltaReport, FileRecord, Plan, Snapshot, SymbolRecord } from "../cli-types.ts";
import type { DependencyReport } from "../dependencies.ts";
import type { DiagnosticSummary } from "../diagnostics.ts";
import { compareText } from "./format.ts";
import { compareDirectory, groupDirectory } from "./groups.ts";
import { computeMetric, isEligibleFile, removableFileLines, removableLinesFor } from "./metrics.ts";
import type {
  CleanupDependency,
  CleanupItem,
  CleanupReport,
  DependencyReportKind,
  PreexistingCounts,
  ProtectedItem,
  ProtectedReason,
  ReportConfidence,
  ReportDiagnostics,
  ReportGroup,
  ReportTrigger,
  TriggerKind,
} from "./types.ts";

interface RuntimeDeltaItem {
  symbol: string;
  file: string;
  estimatedLines: number;
  trigger?: {
    file?: string;
    kind?: string;
    change?: string;
    certainty?: string;
  };
  confidence?: string;
}

interface RuntimeDelta extends DeltaReport {
  newlyDead: RuntimeDeltaItem[];
}

export interface CurrentReportInput {
  snapshot: Snapshot;
  headCommit: string;
  dirty: boolean;
  dependencies: DependencyReport;
  diagnostics: DiagnosticSummary;
}

export interface BranchReportInput {
  head: Snapshot;
  delta: DeltaReport;
  plan: Plan;
  baseRef: string | null;
  baseCommit: string;
  headCommit: string;
  dirty: boolean;
  dependencies: DependencyReport;
  diagnostics: DiagnosticSummary;
}

const TRIGGER_KINDS = new Set<TriggerKind>([
  "removed-import",
  "removed-call",
  "removed-jsx",
  "removed-route",
  "renamed-export",
  "removed-re-export",
  "changed-dynamic",
  "indirect",
]);

const PROTECTED_REASONS = new Set<ProtectedReason>([
  "entry-role",
  "public-api",
  "unknown-edge",
  "probable-edge",
  "unparsed-file",
  "generated-file",
  "test-entry",
  "external-referrer",
  "blast-radius",
  "workspace-boundary",
  "policy",
  "oversized",
  "other",
]);

function triggerKind(value: string | undefined): TriggerKind {
  return value && TRIGGER_KINDS.has(value as TriggerKind) ? (value as TriggerKind) : "indirect";
}

function confidenceOf(value: string | undefined): ReportConfidence {
  return value === "probable" || value === "unknown" ? value : "certain";
}

function fileOfKey(key: string): string {
  return key.split("#", 1)[0] ?? key;
}

/** Normalize an engine protection string to the deletion-plan vocabulary. */
export function normalizeProtectedReason(reason: string): ProtectedReason | null {
  const text = reason.trim().toLowerCase();
  if (PROTECTED_REASONS.has(text as ProtectedReason)) return text as ProtectedReason;
  if (text.includes("public api")) return "public-api";
  if (text.includes("generated")) return "generated-file";
  if (text === "entry" || text.includes("entry role") || text.includes("entry-role")) return "entry-role";
  if (text.includes("probable")) return "probable-edge";
  if (text.includes("unknown")) return "unknown-edge";
  if (text.includes("unparsed") || text.includes("parse error")) return "unparsed-file";
  if (text.includes("test")) return "test-entry";
  if (text.includes("external")) return "external-referrer";
  if (text.includes("blast")) return "blast-radius";
  if (text.includes("workspace")) return "workspace-boundary";
  if (text.includes("oversized")) return "oversized";
  if (text.includes("policy")) return "policy";
  if (text.includes("no witness") || text.includes("isolated")) return null;
  return "other";
}

function compareItems(left: CleanupItem, right: CleanupItem): number {
  return compareText(left.path, right.path)
    || compareText(left.symbol ?? "", right.symbol ?? "")
    || compareText(left.id, right.id);
}

function compareProtected(left: ProtectedItem, right: ProtectedItem): number {
  return compareText(left.path, right.path)
    || compareText(left.symbol ?? "", right.symbol ?? "")
    || compareText(left.reason, right.reason);
}

export function dependencyItems(report: DependencyReport): CleanupDependency[] {
  const items: CleanupDependency[] = [];
  for (const pkg of report.packages) {
    for (const item of pkg.unused) {
      items.push({
        package: pkg.name,
        directory: pkg.directory,
        name: item.name,
        kind: "unused",
        advisory: true,
      });
    }
    for (const item of pkg.unlisted) {
      items.push({
        package: pkg.name,
        directory: pkg.directory,
        name: item.name,
        kind: "unlisted",
        advisory: true,
      });
    }
    for (const item of pkg.transitive) {
      items.push({
        package: pkg.name,
        directory: pkg.directory,
        name: item.name,
        kind: "transitive",
        advisory: true,
      });
    }
  }
  return sortDependencies(items);
}

function dependencyKindRank(kind: DependencyReportKind): number {
  return kind === "unused" ? 0 : kind === "unlisted" ? 1 : 2;
}

export function reportDiagnostics(summary: DiagnosticSummary): ReportDiagnostics {
  return {
    total: summary.total,
    actionable: summary.actionable,
    informational: summary.informational,
    groups: summary.groups.slice(0, 6).map((group) => ({
      category: group.category,
      code: group.code,
      count: group.count,
      actionable: group.actionable,
      files: [...group.files].sort(compareText),
    })),
  };
}

function buildGroups(
  snapshot: Snapshot,
  byKey: ReadonlyMap<string, SymbolRecord>,
  items: CleanupItem[],
  symbolKeys: ReadonlySet<string>,
  wholeFiles: ReadonlySet<string>,
  fileLines: ReadonlyMap<string, number>,
): ReportGroup[] {
  const directoryCache = new Map<string, string>();
  const directoryOfPath = (path: string): string => {
    const cached = directoryCache.get(path);
    if (cached !== undefined) return cached;
    const directory = groupDirectory(path);
    directoryCache.set(path, directory);
    return directory;
  };
  const keysByGroup = new Map<string, Set<string>>();
  const wholeByGroup = new Map<string, Set<string>>();
  const itemsByGroup = new Map<string, CleanupItem[]>();
  for (const key of symbolKeys) {
    if (key.endsWith("#__module__")) continue;
    const symbol = byKey.get(key);
    if (!symbol) continue;
    const directory = directoryOfPath(symbol.file);
    const set = keysByGroup.get(directory) ?? new Set<string>();
    set.add(key);
    keysByGroup.set(directory, set);
  }
  for (const path of wholeFiles) {
    const directory = directoryOfPath(path);
    const set = wholeByGroup.get(directory) ?? new Set<string>();
    set.add(path);
    wholeByGroup.set(directory, set);
  }
  for (const item of items) {
    const directory = directoryOfPath(item.path);
    const list = itemsByGroup.get(directory) ?? [];
    list.push(item);
    itemsByGroup.set(directory, list);
  }
  const directories = new Set([
    ...keysByGroup.keys(),
    ...wholeByGroup.keys(),
    ...itemsByGroup.keys(),
  ]);
  const groups: ReportGroup[] = [];
  for (const directory of directories) {
    const keys = keysByGroup.get(directory) ?? new Set<string>();
    const whole = wholeByGroup.get(directory) ?? new Set<string>();
    const groupItems = [...(itemsByGroup.get(directory) ?? [])].sort(compareItems);
    const filePaths = new Set<string>(whole);
    for (const key of keys) {
      const symbol = byKey.get(key);
      if (symbol) filePaths.add(symbol.file);
    }
    for (const item of groupItems) {
      if (item.kind === "symbol" || item.kind === "file") filePaths.add(item.path);
    }
    let removableLines = 0;
    for (const path of filePaths) removableLines += fileLines.get(path) ?? 0;
    groups.push({
      directory,
      removableLines,
      removableSymbols: keys.size,
      removableFiles: filePaths.size,
      items: groupItems,
    });
  }
  return groups.sort((left, right) => compareDirectory(left.directory, right.directory));
}

function summaryFor(
  metric: ReturnType<typeof computeMetric>,
  dependencies: readonly CleanupDependency[],
  protectedItems: readonly ProtectedItem[],
  diagnostics: DiagnosticSummary,
): CleanupReport["summary"] {
  return {
    analyzedFiles: metric.analyzedFiles,
    analyzedLines: metric.analyzedLines,
    removableFiles: metric.removableFiles,
    removableSymbols: metric.removableSymbols,
    removableExports: metric.removableExports,
    removableLines: metric.removableLines,
    deadWeightBasisPoints: metric.deadWeightBasisPoints,
    unusedDependencies: dependencies.length,
    protectedItems: protectedItems.length,
    uncountedRemovableSymbols: metric.uncountedRemovableSymbols,
    diagnostics: {
      total: diagnostics.total,
      actionable: diagnostics.actionable,
      informational: diagnostics.informational,
    },
  };
}

function fileItem(
  file: FileRecord,
  classification: "never-used" | "branch-orphan",
): CleanupItem {
  return {
    id: `file:${file.path}`,
    kind: "file",
    classification,
    path: file.path,
    lines: file.lines,
    reason: "whole-file",
    confidence: "certain",
  };
}

function symbolItem(
  symbol: SymbolRecord,
  classification: "never-used" | "branch-orphan",
  trigger: ReportTrigger | undefined,
  confidence: ReportConfidence,
): CleanupItem {
  return {
    id: `symbol:${symbol.key}`,
    kind: "symbol",
    classification,
    path: symbol.file,
    symbol: symbol.key,
    name: symbol.name,
    lines: symbol.lines,
    reason: "no-witness",
    confidence,
    ...(trigger ? { trigger } : {}),
  };
}

function addProtected(
  items: ProtectedItem[],
  seen: Set<string>,
  path: string,
  symbol: string | undefined,
  reason: ProtectedReason,
  detail: string | undefined,
): void {
  if (!path) return;
  const key = `${path}\0${symbol ?? ""}\0${reason}\0${detail ?? ""}`;
  if (seen.has(key)) return;
  seen.add(key);
  items.push({
    path,
    ...(symbol ? { symbol } : {}),
    reason,
    ...(detail ? { detail } : {}),
  });
}

function currentProtected(
  snapshot: Snapshot,
  byKey?: ReadonlyMap<string, SymbolRecord>,
): ProtectedItem[] {
  if (
    snapshot.proofs.length === 0
    && snapshot.unknowns.length === 0
    && snapshot.internalOnly.length === 0
  ) {
    return [];
  }
  const items: ProtectedItem[] = [];
  const seen = new Set<string>();
  const symbols = byKey ?? new Map(snapshot.symbols.map((symbol) => [symbol.key, symbol]));
  for (const proof of snapshot.proofs) {
    const reason = normalizeProtectedReason(proof.reason);
    if (!reason) continue;
    const symbol = symbols.get(proof.symbol);
    addProtected(
      items,
      seen,
      symbol?.file ?? fileOfKey(proof.symbol),
      proof.symbol,
      reason,
      proof.reason === reason ? undefined : proof.reason,
    );
  }
  for (const unknown of snapshot.unknowns) {
    addProtected(items, seen, unknown.file, undefined, "unknown-edge", unknown.reason);
  }
  for (const key of snapshot.internalOnly) {
    const symbol = symbols.get(key);
    addProtected(
      items,
      seen,
      symbol?.file ?? fileOfKey(key),
      key,
      "other",
      "live only within its file; not removable",
    );
  }
  return items.sort(compareProtected);
}

function branchProtected(plan: Plan): ProtectedItem[] {
  const items: ProtectedItem[] = [];
  const seen = new Set<string>();
  for (const blocked of plan.blocked) {
    const reason = normalizeProtectedReason(blocked.reason) ?? "other";
    addProtected(
      items,
      seen,
      fileOfKey(blocked.symbol),
      blocked.symbol,
      reason,
      blocked.reason === reason ? undefined : blocked.reason,
    );
  }
  return items.sort(compareProtected);
}

interface SnapshotMaps {
  byPath: ReadonlyMap<string, FileRecord>;
  byKey: ReadonlyMap<string, SymbolRecord>;
}

function eligiblePaths(
  snapshot: Snapshot,
  paths: Iterable<string>,
  maps?: SnapshotMaps,
): Set<string> {
  const byPath = maps?.byPath ?? new Map(snapshot.files.map((file) => [file.path, file]));
  const result = new Set<string>();
  for (const path of paths) {
    const file = byPath.get(path);
    if (file && isEligibleFile(file)) result.add(path);
  }
  return result;
}

/**
 * Candidate keys the report can display: the symbol exists in the snapshot and
 * its file can be removed. Anything else stays in the metric as an uncounted
 * removable symbol instead of becoming a zero-line group.
 */
function eligibleSymbolKeys(
  snapshot: Snapshot,
  keys: Iterable<string>,
  maps?: SnapshotMaps,
): Set<string> {
  const byPath = maps?.byPath ?? new Map(snapshot.files.map((file) => [file.path, file]));
  const byKey = maps?.byKey ?? new Map(snapshot.symbols.map((symbol) => [symbol.key, symbol]));
  const result = new Set<string>();
  for (const key of keys) {
    if (key.endsWith("#__module__")) continue;
    const symbol = byKey.get(key);
    const file = symbol ? byPath.get(symbol.file) : undefined;
    if (symbol && file && isEligibleFile(file)) result.add(key);
  }
  return result;
}

function wholeFilesFor(
  snapshot: Snapshot,
  symbolKeys: ReadonlySet<string>,
  maps?: SnapshotMaps,
): Set<string> {
  const candidates = new Set(snapshot.unusedFiles);
  let moduleKeys = false;
  for (const key of symbolKeys) {
    if (!key.endsWith("#__module__")) continue;
    moduleKeys = true;
    break;
  }
  if (!moduleKeys && candidates.size === 0) return new Set();
  const byKey = maps?.byKey ?? new Map(snapshot.symbols.map((symbol) => [symbol.key, symbol]));
  if (moduleKeys) {
    for (const key of symbolKeys) {
      if (!key.endsWith("#__module__")) continue;
      const symbol = byKey.get(key);
      if (symbol) candidates.add(symbol.file);
    }
  }
  return eligiblePaths(snapshot, candidates, maps);
}

function preexistingCounts(
  snapshot: Snapshot,
  plannedSymbols: ReadonlySet<string>,
  plannedFiles: ReadonlySet<string>,
  maps?: SnapshotMaps,
): PreexistingCounts {
  const keys = eligibleSymbolKeys(
    snapshot,
    snapshot.candidates.filter((key) => !plannedSymbols.has(key)),
    maps,
  );
  const whole = eligiblePaths(
    snapshot,
    snapshot.unusedFiles.filter((path) => !plannedFiles.has(path)),
    maps,
  );
  const byKey = maps?.byKey ?? new Map(snapshot.symbols.map((symbol) => [symbol.key, symbol]));
  const files = new Set<string>(whole);
  for (const key of keys) {
    const symbol = byKey.get(key);
    if (symbol) files.add(symbol.file);
  }
  return {
    symbols: keys.size,
    files: files.size,
    lines: removableLinesFor(snapshot, keys, whole),
  };
}

export function buildCurrentReport(input: CurrentReportInput): CleanupReport {
  const { snapshot } = input;
  const byKey = new Map(snapshot.symbols.map((symbol) => [symbol.key, symbol]));
  const byPath = new Map(snapshot.files.map((file) => [file.path, file]));
  const maps: SnapshotMaps = { byPath, byKey };
  const candidateKeys = new Set(
    snapshot.candidates.filter((key) => !key.endsWith("#__module__")),
  );
  const symbolKeys = eligibleSymbolKeys(snapshot, candidateKeys, maps);
  const wholeFiles = wholeFilesFor(snapshot, new Set(snapshot.candidates), maps);
  const items: CleanupItem[] = [];
  for (const file of snapshot.files) {
    if (wholeFiles.has(file.path)) items.push(fileItem(file, "never-used"));
  }
  for (const key of symbolKeys) {
    const symbol = byKey.get(key);
    if (!symbol || wholeFiles.has(symbol.file)) continue;
    items.push(symbolItem(symbol, "never-used", undefined, "certain"));
  }
  const removableFiles = removableFileLines(snapshot, symbolKeys, wholeFiles);
  const metric = computeMetric(snapshot, candidateKeys, wholeFiles, removableFiles, byKey);
  const fileLines = new Map<string, number>();
  for (const [path, entry] of removableFiles) {
    fileLines.set(path, entry.lines);
  }
  const dependencies = dependencyItems(input.dependencies);
  const protectedItems = currentProtected(snapshot, byKey);
  const diagnostics = reportDiagnostics(input.diagnostics);
  return {
    schemaVersion: "1.0",
    mode: "current",
    base: null,
    head: {
      commit: input.headCommit,
      dirty: input.dirty,
      graphHash: snapshot.graphHash,
    },
    summary: summaryFor(metric, dependencies, protectedItems, input.diagnostics),
    groups: buildGroups(snapshot, byKey, items, symbolKeys, wholeFiles, fileLines),
    protected: protectedItems,
    dependencies,
    diagnostics,
  };
}

export function buildBranchReport(input: BranchReportInput): CleanupReport {
  const { head, plan } = input;
  const delta = input.delta as RuntimeDelta;
  const planSymbols = new Set(plan.batches.flatMap((batch) => batch.symbols));
  const byKey = new Map(head.symbols.map((symbol) => [symbol.key, symbol]));
  const byPath = new Map(head.files.map((file) => [file.path, file]));
  const maps: SnapshotMaps = { byPath, byKey };
  const wholeCandidates = new Set(delta.newlyDeadFiles);
  for (const key of planSymbols) {
    if (!key.endsWith("#__module__")) continue;
    const symbol = byKey.get(key);
    if (symbol) wholeCandidates.add(symbol.file);
  }
  const wholeFiles = eligiblePaths(head, wholeCandidates, maps);
  const planSymbolKeys = new Set(
    [...planSymbols].filter((key) => !key.endsWith("#__module__")),
  );
  const symbolKeys = eligibleSymbolKeys(head, planSymbolKeys, maps);
  const deltaItems = new Map(delta.newlyDead.map((item) => [item.symbol, item]));
  const items: CleanupItem[] = [];
  for (const file of head.files) {
    if (wholeFiles.has(file.path)) items.push(fileItem(file, "branch-orphan"));
  }
  for (const key of symbolKeys) {
    const symbol = byKey.get(key);
    if (!symbol || wholeFiles.has(symbol.file)) continue;
    const item = deltaItems.get(key);
    const trigger: ReportTrigger = item
      ? {
          file: item.trigger?.file || item.file,
          kind: triggerKind(item.trigger?.kind),
          change: item.trigger?.change || "no single edge explains the lost witness",
          certainty: confidenceOf(item.trigger?.certainty ?? item.confidence),
        }
      : {
          file: symbol.file,
          kind: "indirect",
          change: "cascade from a branch orphan",
          certainty: "certain",
        };
    const confidence = item ? confidenceOf(item.confidence) : "certain";
    items.push(symbolItem(symbol, "branch-orphan", trigger, confidence));
  }
  const removableFiles = removableFileLines(head, symbolKeys, wholeFiles);
  const metric = computeMetric(head, planSymbolKeys, wholeFiles, removableFiles, byKey);
  const fileLines = new Map<string, number>();
  for (const [path, entry] of removableFiles) {
    fileLines.set(path, entry.lines);
  }
  const plannedFiles = new Set(plan.batches.flatMap((batch) => batch.files));
  const dependencies = dependencyItems(input.dependencies);
  const protectedItems = branchProtected(plan);
  const diagnostics = reportDiagnostics(input.diagnostics);
  return {
    schemaVersion: "1.0",
    mode: "branch",
    base: {
      ref: input.baseRef,
      commit: input.baseCommit,
    },
    head: {
      commit: input.headCommit,
      dirty: input.dirty,
      graphHash: head.graphHash,
    },
    summary: summaryFor(metric, dependencies, protectedItems, input.diagnostics),
    groups: buildGroups(head, byKey, items, symbolKeys, wholeFiles, fileLines),
    preexisting: preexistingCounts(head, planSymbols, plannedFiles, maps),
    protected: protectedItems,
    dependencies,
    diagnostics,
  };
}

export function reportExitCode(report: CleanupReport): number {
  return report.summary.removableFiles > 0 || report.summary.removableSymbols > 0 ? 1 : 0;
}

export function firstWhySymbol(report: CleanupReport): string | null {
  for (const group of report.groups) {
    for (const item of group.items) {
      if (item.kind === "symbol" && item.symbol) return item.symbol;
    }
  }
  return null;
}

export function sortDependencies(items: CleanupDependency[]): CleanupDependency[] {
  return [...items].sort(
    (left, right) =>
      compareText(left.package, right.package)
      || compareText(left.name, right.name)
      || dependencyKindRank(left.kind) - dependencyKindRank(right.kind)
      || compareText(left.directory, right.directory),
  );
}
