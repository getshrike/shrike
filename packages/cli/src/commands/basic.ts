import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { booleanFlag, rejectUnknownFlags, stringFlag, type ParsedArgs } from "../args.ts";
import { loadConfig, engineConfig, cacheDirectory } from "../config.ts";
import { requestEngine } from "../engine.ts";
import { CliError, ExitCode, RevertFailedError, UsageError } from "../errors.ts";
import { formatCount, writeHuman, writeJson } from "../output.ts";
import { RecoveryError, restoreRecoverySnapshot } from "../recovery.ts";
import { VERSION, PROTOCOL_VERSION } from "../version.ts";
import { loadBuildEvidence } from "../build.ts";
import { readReceipt, listReceipts } from "@shrike/receipts";
import { analyze, analyzePair, toReceiptBuild } from "../session.ts";
import { storePlan } from "../plan-store.ts";
import { renderDelta, renderPlan } from "../render.ts";
import { analyzeDependencies, type DependencyReport } from "../dependencies.ts";
import { summarizeDiagnostics, type DiagnosticSummary } from "../diagnostics.ts";
import type { BuildJoinResult, Plan, ScanResult, Snapshot } from "../cli-types.ts";

export function versionCommand(args: ParsedArgs): number {
  const value = { name: "shrike", version: VERSION, protocolVersion: PROTOCOL_VERSION, engine: "rust" };
  if (booleanFlag(args, "json")) writeJson(value);
  else writeHuman(`shrike ${VERSION} (protocol ${PROTOCOL_VERSION})`);
  return ExitCode.OK;
}

export function initCommand(root: string, args: ParsedArgs): number {
  const force = booleanFlag(args, "force");
  const path = join(root, "shrike.config.ts");
  if (existsSync(path) && !force) throw new UsageError("shrike.config.ts already exists; pass --force to replace it");
  const config = `import type { ShrikeConfig } from "shrike/config";

/** Shrike configuration. See schemas/config.schema.json. */
const config: Partial<ShrikeConfig> = {
  entryPolicy: "local",
  testEntries: true,
  includeGenerated: false,
  batchPolicy: { maxSymbols: 50, maxFiles: 10, maxLiveGraphPercent: 5, allowOversized: false },
  checks: [],
};

export default config;
`;
  writeFileSync(path, config);
  const ignorePath = join(root, ".gitignore");
  const existing = existsSync(ignorePath) ? readFileSync(ignorePath, "utf8") : "";
  if (!existing.split("\n").includes(".shrike/")) {
    writeFileSync(ignorePath, `${existing.endsWith("\n") || existing.length === 0 ? existing : `${existing}\n`}.shrike/\n`);
  }
  if (booleanFlag(args, "json")) writeJson({ created: ["shrike.config.ts", ".gitignore"] });
  else writeHuman("Created shrike.config.ts and added .shrike/ to .gitignore.");
  return ExitCode.OK;
}
export async function scanCommand(root: string, args: ParsedArgs): Promise<number> {
  const config = await loadConfig(root);
  const result = await requestEngine<ScanResult>("scan", {
    root,
    config: engineConfig(config),
    cacheDir: cacheDirectory(root, config),
  });
  const dependencies = analyzeDependencies(root, result.snapshot, config.ignoreDependencies);
  const diagnostics = summarizeDiagnostics(result.snapshot.diagnostics ?? []);
  const payload = { ...result, dependencies, diagnosticsSummary: diagnostics };
  if (booleanFlag(args, "json")) writeJson(payload);
  else {
    writeHuman(`Shrike scan: ${formatCount(result.candidates.length, "candidate")} (${result.snapshot.files.length} files)`);
    const incoming = new Map<string, number>();
    for (const edge of result.snapshot.edges ?? []) {
      incoming.set(edge.target, (incoming.get(edge.target) ?? 0) + 1);
    }
    for (const item of result.candidates) {
      const references = incoming.get(item.symbol) ?? 0;
      const evidence = references > 0
        ? `only dead references (${references})`
        : "no incoming references";
      writeHuman(`  ${item.symbol}  -${item.lines} lines  [${evidence}]`);
    }
    const unusedFiles = result.snapshot.unusedFiles ?? [];
    if (unusedFiles.length > 0) {
      writeHuman(`Unused files: ${formatCount(unusedFiles.length, "file")} (no entry or import)`);
      for (const file of unusedFiles) writeHuman(`  ${file}  (file)`);
    }
    const internalOnly = result.snapshot.internalOnly ?? [];
    if (internalOnly.length > 0) {
      writeHuman(
        `Exported but only used internally: ${formatCount(internalOnly.length, "export")} (not deletion candidates)`,
      );
      for (const symbol of internalOnly.slice(0, 20)) writeHuman(`  ${symbol}`);
      if (internalOnly.length > 20) writeHuman(`  ... ${internalOnly.length - 20} more`);
    }
    renderDiagnostics(diagnostics);
    renderDependencies(dependencies);
  }
  return result.candidates.length > 0
    || (result.snapshot.unusedFiles?.length ?? 0) > 0
    || dependencies.summary.unused > 0
    || dependencies.summary.unlisted > 0
    ? ExitCode.FINDINGS
    : ExitCode.OK;
}

export async function depsCommand(root: string, args: ParsedArgs): Promise<number> {
  rejectUnknownFlags(args, ["json", "help"]);
  const config = await loadConfig(root);
  const result = await requestEngine<ScanResult>("scan", {
    root,
    config: engineConfig(config),
    cacheDir: cacheDirectory(root, config),
  });
  const dependencies = analyzeDependencies(root, result.snapshot, config.ignoreDependencies);
  if (booleanFlag(args, "json")) writeJson(dependencies);
  else renderDependencies(dependencies);
  return dependencies.summary.unused > 0 || dependencies.summary.unlisted > 0
    ? ExitCode.FINDINGS
    : ExitCode.OK;
}

function renderDependencies(report: DependencyReport): void {
  const { summary } = report;
  if (summary.unused === 0 && summary.unlisted === 0 && summary.transitive === 0) {
    writeHuman("Dependencies: none unused or unlisted.");
    return;
  }
  writeHuman(
    `Dependencies: ${formatCount(summary.unused, "unused")}, ${formatCount(summary.unlisted, "unlisted")}` +
      (summary.transitive > 0 ? `, ${formatCount(summary.transitive, "transitive")}` : ""),
  );
  for (const pkg of report.packages) {
    const label = pkg.directory === "" ? pkg.name : `${pkg.name} (${pkg.directory})`;
    for (const item of pkg.unused) {
      writeHuman(`  ${label}: ${item.name}  [${item.kind}]`);
    }
    for (const item of pkg.unlisted) {
      writeHuman(`  ${label}: ${item.name}  [imported but not declared]`);
    }
    for (const item of pkg.transitive) {
      writeHuman(`  ${label}: ${item.name}  [provided by a declared dependency; informational]`);
    }
  }
}

function renderDiagnostics(summary: DiagnosticSummary): void {
  if (summary.total === 0) return;
  writeHuman(
    `Diagnostics: ${summary.total} (${summary.actionable} actionable, ${summary.informational} informational)`,
  );
  for (const group of summary.groups.slice(0, 6)) {
    const label = group.actionable ? group.category : `${group.category} (info)`;
    writeHuman(`  ${label}: ${group.count} × ${group.code}${group.files[0] ? `  [${group.files[0]}]` : ""}`);
  }
}
export async function deltaCommand(root: string, args: ParsedArgs): Promise<number> {
  const failOn = stringFlag(args, "fail-on") ?? "newly-dead";
  if (failOn !== "newly-dead" && failOn !== "never") {
    throw new UsageError(`invalid --fail-on value: ${failOn}; expected newly-dead or never`);
  }
  // analyzePair rejects unknown flags; consume this one first so the shared
  // analyzer does not need to know about delta-only policy.
  args.values.delete("fail-on");
  const pair = await analyzePair(root, args);
  try {
    if (booleanFlag(args, "json")) writeJson(pair.delta);
    else renderDelta(pair.delta);
    if (failOn === "never") return ExitCode.OK;
    return pair.delta.newlyDead.length > 0 || pair.delta.newlyDeadFiles.length > 0
      ? ExitCode.FINDINGS : ExitCode.OK;
  } finally { pair.cleanup(); }
}
export async function planCommand(root: string, args: ParsedArgs): Promise<number> {
  const pair = await analyzePair(root, args);
  try {
    const config = await loadConfig(root);
    const plan = await requestEngine<Plan>("plan", { snapshot: pair.head, delta: pair.delta, config: engineConfig(config) });
    mkdirSync(join(root, ".shrike"), { recursive: true });
    writeFileSync(join(root, ".shrike", "last-plan.json"), `${JSON.stringify(plan, null, 2)}\n`);
    storePlan(root, pair, plan, config.verification.tier);
    if (booleanFlag(args, "json")) writeJson(plan);
    else renderPlan(plan);
    return plan.totals.symbols > 0 ? ExitCode.FINDINGS : ExitCode.OK;
  } finally { pair.cleanup(); }
}
export function receiptCommand(root: string, args: ParsedArgs): number {
  const id = args.positionals[0];
  if (!id) {
    const receipts = listReceipts(root);
    if (booleanFlag(args, "json")) writeJson({ receipts });
    else writeHuman(receipts.length > 0 ? receipts.join("\n") : "No receipts.");
    return ExitCode.OK;
  }
  if (!/^rcpt_[A-Za-z0-9]{8,64}$/.test(id)) {
    throw new UsageError("receipt id must look like rcpt_<hex>");
  }
  const receipt = readReceipt(join(root, ".shrike", "receipts", `${id}.json`));
  if (booleanFlag(args, "json")) writeJson(receipt);
  else writeHuman(`${receipt.id} ${receipt.outcome} ${receipt.createdAt}`);
  return ExitCode.OK;
}

export function recoverCommand(root: string, args: ParsedArgs): number {
  const input = args.positionals[0];
  if (!input) throw new UsageError("recover requires a recovery snapshot path");
  try {
    const result = restoreRecoverySnapshot(root, input);
    if (booleanFlag(args, "json")) {
      writeJson({ restored: result.restored, snapshot: result.snapshot.id });
    } else {
      writeHuman(`Restored ${formatCount(result.restored.length, "file")} from ${input}.`);
    }
    return ExitCode.OK;
  } catch (error) {
    if (error instanceof RecoveryError) {
      throw new RevertFailedError(error.message, error.snapshotPath ?? input);
    }
    throw error;
  }
}

export async function proofCommand(root: string, args: ParsedArgs): Promise<number> {
  const symbol = args.positionals[0];
  if (!symbol) throw new UsageError("proof requires a symbol key (path#Name)");
  const config = await loadConfig(root);
  const snapshot = await analyze(root, config);
  const proof = await requestEngine("proof", { snapshot, symbol });
  if (booleanFlag(args, "json")) writeJson(proof);
  else writeHuman(JSON.stringify(proof, null, 2));
  return ExitCode.OK;
}

export async function evidenceCommand(root: string, args: ParsedArgs): Promise<number> {
  rejectUnknownFlags(args, [
    "json", "build", "require-build-evidence", "help", "run-build",
    "build-command", "no-client-boundary-leaks",
  ]);
  const loaded = loadBuildEvidence(root, stringFlag(args, "build"), {
    runBuild: booleanFlag(args, "run-build"),
    buildCommand: stringFlag(args, "build-command"),
  });
  if (loaded.error) {
    throw new CliError(ExitCode.COULD_NOT_RUN, "build-evidence", loaded.error);
  }
  if (booleanFlag(args, "no-client-boundary-leaks")) {
    loaded.evidence.clientBoundaryLeaks = false;
  }
  const config = await loadConfig(root);
  const snapshot = await analyze(root, config);
  const result = await requestEngine<BuildJoinResult>("join-build", {
    snapshot,
    build: loaded.evidence,
  });
  if (booleanFlag(args, "require-build-evidence") && !["full", "partial"].includes(result.status)) {
    const reason = loaded.failure
      ? `build failed: ${loaded.failure}`
      : `status is ${result.status}`;
    throw new CliError(
      ExitCode.COULD_NOT_RUN,
      "build-evidence-required",
      `build evidence is required but ${reason}`,
    );
  }
  const payload = {
    ...result,
    evidence: {
      path: loaded.path,
      source: loaded.evidence.source,
      version: loaded.evidence.version,
      snapshotHash: loaded.evidence.snapshotHash,
      unknownRecords: loaded.evidence.unknownRecords,
      ...(loaded.failure ? { failure: loaded.failure } : {}),
    },
  };
  if (booleanFlag(args, "json")) writeJson(payload);
  else writeHuman(JSON.stringify(payload, null, 2));
  return ExitCode.OK;
}
export async function cacheCommand(root: string, args: ParsedArgs): Promise<number> {
  const action = args.positionals[0];
  const config = await loadConfig(root);
  const cacheDir = cacheDirectory(root, config);
  if (action === "clean") {
    rmSync(cacheDir, { recursive: true, force: true });
    if (booleanFlag(args, "json")) writeJson({ cleaned: cacheDir });
    else writeHuman("Cache cleaned.");
    return ExitCode.OK;
  }
  if (action === "verify") {
    const snapshots = join(cacheDir, "snapshots");
    const entries = existsSync(snapshots)
      ? readdirSync(snapshots).filter((name) => name.endsWith(".json"))
      : [];
    const broken: string[] = [];
    for (const name of entries) {
      try {
        const parsed = JSON.parse(readFileSync(join(snapshots, name), "utf8")) as { schemaVersion?: string };
        if (parsed.schemaVersion !== "1.0") broken.push(name);
      } catch {
        broken.push(name);
      }
    }
    const result = { path: cacheDir, entries: entries.length, broken };
    if (booleanFlag(args, "json")) writeJson(result);
    else writeHuman(entries.length === 0 ? "Cache is empty." : `Cache: ${entries.length} snapshot(s), ${broken.length} broken.`);
    return broken.length === 0 ? ExitCode.OK : ExitCode.FINDINGS;
  }
  throw new UsageError("usage: shrike cache clean|verify");
}
