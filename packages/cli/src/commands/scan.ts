import { booleanFlag, rejectUnknownFlags, type ParsedArgs } from "../args.ts";
import { cacheDirectory, engineConfig, loadConfig } from "../config.ts";
import { parseEngineLine, requestEngineRaw } from "../engine.ts";
import { ExitCode } from "../errors.ts";
import { formatCount, writeHuman, writeJson, writeRawJson } from "../output.ts";
import {
  analyzeDependencies, type DependencyReport, type SnapshotLike,
} from "../dependencies.ts";
import { logTiming } from "../timing.ts";
import { summarizeDiagnostics, type DiagnosticSummary } from "../diagnostics.ts";
import type { ScanResult, Snapshot } from "../cli-types.ts";

/**
 * The engine's compact dependency projection.
 *
 * Dependency analysis needs module specifiers, entry frameworks, and module
 * liveness. It does not need the full snapshot: edges, witnesses, proofs, and
 * every symbol record stay in the already-serialized result. Sending the
 * projection separately lets the CLI answer a scan without decoding tens of
 * megabytes of JSON.
 */
interface DependencyInput {
  files: string[];
  modules: {
    path: string;
    specifiers: string[];
    external?: string[];
    typeRefs?: string[];
  }[];
  entries: { symbol: string; framework: string }[];
  moduleKeys: string[];
  liveModules: string[];
  diagnostics: NonNullable<Snapshot["diagnostics"]>;
  candidateCount: number;
  unusedFileCount: number;
}

interface FastScan {
  resultPrefix: Buffer;
  dependencies: DependencyReport;
  diagnosticsSummary: DiagnosticSummary;
  candidateCount: number;
  unusedFileCount: number;
}

const DEPENDENCY_MARKER = Buffer.from(',"dependencyInput":');
const ENVELOPE_SUFFIX = Buffer.from('},"error":null}');

/**
 * Split a raw scan response into its serialized result and its compact
 * dependency projection. Returns null for responses from older engines or test
 * doubles, which fall back to the full decode path.
 *
 * The response stays a Buffer: the snapshot is tens of megabytes and is copied
 * straight to stdout without decoding or re-encoding it.
 */
function splitScanResponse(line: Buffer, id: string): {
  resultPrefix: Buffer;
  input: DependencyInput;
} | null {
  const prefix = Buffer.from(`{"protocolVersion":"1.0","id":${JSON.stringify(id)},"ok":true,"result":`);
  if (line.length < prefix.length || !line.subarray(0, prefix.length).equals(prefix)) return null;
  const marker = line.lastIndexOf(DEPENDENCY_MARKER);
  if (marker < prefix.length) return null;
  const resultEnd = line.lastIndexOf(ENVELOPE_SUFFIX);
  if (resultEnd <= marker) return null;
  let input: DependencyInput;
  try {
    input = JSON.parse(
      line.subarray(marker + DEPENDENCY_MARKER.length, resultEnd).toString("utf8"),
    ) as DependencyInput;
  } catch {
    return null;
  }
  if (
    typeof input !== "object" || input === null ||
    !Array.isArray(input.files) || !Array.isArray(input.modules) ||
    !Array.isArray(input.entries) || !Array.isArray(input.moduleKeys) ||
    !Array.isArray(input.liveModules) || !Array.isArray(input.diagnostics) ||
    typeof input.candidateCount !== "number" || typeof input.unusedFileCount !== "number"
  ) {
    return null;
  }
  return { resultPrefix: line.subarray(prefix.length, marker), input };
}

function projectionSnapshot(input: DependencyInput): SnapshotLike {
  return {
    files: input.files.map((path) => ({ path })),
    modules: input.modules,
    entries: input.entries,
    symbols: input.moduleKeys.map((key) => ({ key, kind: "module" })),
    live: input.liveModules,
  };
}

function fastScan(
  line: Buffer,
  id: string,
  root: string,
  config: Awaited<ReturnType<typeof loadConfig>>,
): FastScan | null {
  const split = splitScanResponse(line, id);
  if (!split) return null;
  const started = performance.now();
  const dependencies = analyzeDependencies(
    root,
    projectionSnapshot(split.input),
    config.ignoreDependencies,
  );
  logTiming("dependencies", started);
  return {
    resultPrefix: split.resultPrefix,
    dependencies,
    diagnosticsSummary: summarizeDiagnostics(split.input.diagnostics),
    candidateCount: split.input.candidateCount,
    unusedFileCount: split.input.unusedFileCount,
  };
}

function scanExitCode(
  candidateCount: number,
  unusedFileCount: number,
  dependencies: DependencyReport,
): number {
  return candidateCount > 0
    || unusedFileCount > 0
    || dependencies.summary.unused > 0
    || dependencies.summary.unlisted > 0
    ? ExitCode.FINDINGS
    : ExitCode.OK;
}

export async function scanCommand(root: string, args: ParsedArgs): Promise<number> {
  const started = performance.now();
  const config = await loadConfig(root);
  logTiming("config", started);
  const engineStarted = performance.now();
  const raw = await requestEngineRaw("scan", {
    root,
    config: engineConfig(config),
    cacheDir: cacheDirectory(root, config),
  });
  logTiming("engine", engineStarted);
  const fast = fastScan(raw.line, raw.id, root, config);
  if (fast) {
    if (booleanFlag(args, "json")) {
      const outputStarted = performance.now();
      writeRawJson([
        fast.resultPrefix,
        `,"dependencies":${JSON.stringify(fast.dependencies)}`,
        `,"diagnosticsSummary":${JSON.stringify(fast.diagnosticsSummary)}}\n`,
      ]);
      logTiming("output", outputStarted);
    } else {
      const result = parseEngineLine<ScanResult>(raw.line, raw.id);
      renderScan(result, fast.dependencies, fast.diagnosticsSummary);
    }
    return scanExitCode(fast.candidateCount, fast.unusedFileCount, fast.dependencies);
  }
  const result = parseEngineLine<ScanResult>(raw.line, raw.id);
  const depsStarted = performance.now();
  const dependencies = analyzeDependencies(root, result.snapshot, config.ignoreDependencies);
  logTiming("dependencies", depsStarted);
  const diagnosticsSummary = summarizeDiagnostics(result.snapshot.diagnostics ?? []);
  if (booleanFlag(args, "json")) {
    writeJson({ ...result, dependencies, diagnosticsSummary });
  } else {
    renderScan(result, dependencies, diagnosticsSummary);
  }
  return scanExitCode(
    result.candidates.length,
    result.snapshot.unusedFiles?.length ?? 0,
    dependencies,
  );
}

function renderScan(
  result: ScanResult,
  dependencies: DependencyReport,
  diagnosticsSummary: DiagnosticSummary,
): void {
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
  renderDiagnostics(diagnosticsSummary);
  renderDependencies(dependencies);
}

export async function depsCommand(root: string, args: ParsedArgs): Promise<number> {
  rejectUnknownFlags(args, ["json", "help"]);
  const config = await loadConfig(root);
  const raw = await requestEngineRaw("scan", {
    root,
    config: engineConfig(config),
    cacheDir: cacheDirectory(root, config),
  });
  const fast = fastScan(raw.line, raw.id, root, config);
  let dependencies: DependencyReport;
  if (fast) {
    dependencies = fast.dependencies;
  } else {
    const result = parseEngineLine<ScanResult>(raw.line, raw.id);
    dependencies = analyzeDependencies(root, result.snapshot, config.ignoreDependencies);
  }
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
