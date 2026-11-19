#!/usr/bin/env node
import { execFileSync, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { createInterface } from "node:readline";
import { parseArgs, booleanFlag, stringFlag, rejectUnknownFlags, type ParsedArgs } from "./args.ts";
import {
  loadConfig, engineConfig, findConfigPath, repoRoot, cacheDirectory, type ShrikeConfig,
  type VerificationTier,
} from "./config.ts";
import { requestEngine } from "./engine.ts";
import { ExitCode, isCliError, UsageError, ConfigError, CliError, RevertFailedError } from "./errors.ts";
import { writeJson, writeHuman, writeStderr, writeJsonError, formatCount } from "./output.ts";
import {
  RecoveryError,
  resolveInside,
  restoreRecoverySnapshot,
  safeRelativePath,
  writeRecoverySnapshot,
  type RecoveryReference,
} from "./recovery.ts";
import {
  createBranchWorktree, createWorktree, ensureBranch, commitAll, git, headCommit, isDirty, mergeBase,
  resolveDefaultBase, restoreAll,
} from "./git.ts";
import { VERSION, PROTOCOL_VERSION } from "./version.ts";
import { EMPTY_BUILD_HASH, loadBuildEvidence, type BuildEvidence } from "./build.ts";
import {
  bisectFailure, captureBaseline, classifyCheck, detectPackageManager, detectRepositoryChecks, runChecks,
  runCheckWithReruns,
  type Baseline, type BisectUnit, type CheckResult, type CheckSpec, type ComparedCheck,
} from "@shrike/verifier";
import {
  buildReceipt, writeReceipt, readReceipt, listReceipts, captureEnvironment, toReceiptChecks,
  renderPrBody, renderReceiptComment,
  type ReceiptBuild, type ReceiptRejection,
} from "@shrike/receipts";
import { buildCleanupBrief, installHook, uninstallHook, type HookTarget } from "@shrike/agent";

interface Snapshot {
  files: { path: string; contentHash: string; lines: number }[];
  symbols: { key: string; file: string; kind: string; span: { start: number; end: number; line: number }; lines: number; publicApi: boolean }[];
  candidates: string[];
  live?: string[];
  graphHash: string;
  [key: string]: unknown;
}
interface DeltaReport {
  base: { ref: string | null; commit: string };
  head: { commit: string; dirty: boolean };
  newlyDead: { symbol: string; file: string; estimatedLines: number }[];
  newlyDeadFiles: string[];
  cascade: { symbols: number; files: number; lines: number };
  batches: { id: string; symbols: number; files: number; lines: number }[];
  incomparable: unknown;
  [key: string]: unknown;
}
interface Plan {
  id: string;
  deltaHash: string;
  base: { ref: string | null; commit: string };
  head: { commit: string; dirty: boolean };
  totals: { symbols: number; files: number; lines: number };
  batches: { id: string; symbols: string[]; files: string[]; lines: number; patch: string }[];
  blocked: { symbol: string; reason: string }[];
  policy?: { entryPolicy: string; maxSymbols: number; maxFiles: number; allowOversized: boolean };
  [key: string]: unknown;
}

interface BuildJoinResult {
  status: string;
  joined: number;
  total: number;
  coverage: number;
  unmatched: string[];
  routes: { route: string; files: number; symbols: number; bytes: number }[];
  bytes: { matched: number; shared: number; totalRoute: number };
  diagnostics: { code: string; severity: string; message: string }[];
}

interface BuildEvidenceInfo {
  evidence: BuildEvidence;
  join: BuildJoinResult;
  path: string | null;
}

const BOOLEAN_FLAGS = new Set([
  "json", "help", "force", "pr", "in-place", "allow-dirty", "verify", "receipt",
  "quiet", "dry-run", "claude", "cursor", "opencode", "all", "no-checks",
  "require-build-evidence",
]);

async function main(argv: string[]): Promise<number> {
  const command = argv[0];
  if (!command || command === "--help" || command === "-h" || command === "help") {
    printHelp();
    return ExitCode.OK;
  }
  const args = parseArgs(argv.slice(1), BOOLEAN_FLAGS);
  if (booleanFlag(args, "help")) { printHelp(); return ExitCode.OK; }
  const root = repoRoot();
  switch (command) {
    case "version": return versionCommand(args);
    case "init": return initCommand(root, args);
    case "scan": return scanCommand(root, args);
    case "delta": return deltaCommand(root, args);
    case "plan": return planCommand(root, args);
    case "prune": return pruneCommand(root, args);
    case "verify": return verifyCommand(root, args);
    case "receipt": return receiptCommand(root, args);
    case "recover": return recoverCommand(root, args);
    case "proof": return proofCommand(root, args);
    case "evidence": return evidenceCommand(root, args);
    case "hook": return hookCommand(root, args);
    case "mcp": return mcpCommand(root, args);
    case "cache": return cacheCommand(root, args);
    default: throw new UsageError(`unknown command: ${command}`);
  }
}

function versionCommand(args: ParsedArgs): number {
  const value = { name: "shrike", version: VERSION, protocolVersion: PROTOCOL_VERSION, engine: "rust" };
  if (booleanFlag(args, "json")) writeJson(value);
  else writeHuman(`shrike ${VERSION} (protocol ${PROTOCOL_VERSION})`);
  return ExitCode.OK;
}

function initCommand(root: string, args: ParsedArgs): number {
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

async function analyze(root: string, config: ShrikeConfig): Promise<Snapshot> {
  const result = await requestEngine<{ snapshot: Snapshot }>("analyze", {
    root,
    config: engineConfig(config),
    cacheDir: cacheDirectory(root, config),
  });
  return result.snapshot;
}

async function scanCommand(root: string, args: ParsedArgs): Promise<number> {
  const config = await loadConfig(root);
  const result = await requestEngine<{ snapshot: Snapshot; candidates: unknown[] }>("scan", {
    root,
    config: engineConfig(config),
    cacheDir: cacheDirectory(root, config),
  });
  if (booleanFlag(args, "json")) writeJson(result);
  else {
    writeHuman(`Shrike scan: ${formatCount(result.candidates.length, "candidate")} (${result.snapshot.files.length} files)`);
    for (const item of result.candidates as { symbol: string; lines: number }[]) {
      writeHuman(`  ${item.symbol}  -${item.lines} lines`);
    }
  }
  return result.candidates.length > 0 ? ExitCode.FINDINGS : ExitCode.OK;
}

interface AnalysisPair {
  base: Snapshot;
  head: Snapshot;
  delta: DeltaReport;
  baseRef: string;
  baseCommit: string;
  headCommit: string;
  dirty: boolean;
  build: BuildEvidenceInfo | null;
  cleanup: () => void;
}

async function analyzePair(
  root: string,
  args: ParsedArgs,
  options: { allowDirty?: boolean } = {},
): Promise<AnalysisPair> {
  rejectUnknownFlags(args, [
    "json", "base", "help", "allow-dirty", "quiet", "build", "require-build-evidence",
    "run-build", "build-command", "no-client-boundary-leaks", "tier", "in-place",
  ]);
  const config = await loadConfig(root);
  const baseRef = resolveDefaultBase(root, stringFlag(args, "base"));
  const baseCommit = mergeBase(root, baseRef);
  const head = headCommit(root);
  const dirty = isDirty(root);
  if (dirty && !(booleanFlag(args, "allow-dirty") || options.allowDirty)) {
    throw new UsageError("working tree is dirty; commit, stash, or pass --allow-dirty");
  }
  const worktree = createWorktree(root, baseCommit, "base");
  try {
    const base = await analyze(worktree.directory, config);
    const headSnapshot = await analyze(root, config);
    const build = await attachBuildEvidence(root, headSnapshot, args, config);
    const delta = await requestEngine<DeltaReport>("delta", {
      baseSnapshot: base, headSnapshot, baseRef, baseCommit, headCommit: head, dirty,
    });
    return { base, head: headSnapshot, delta, baseRef, baseCommit, headCommit: head, dirty, build, cleanup: worktree.cleanup };
  } catch (error) {
    worktree.cleanup();
    throw error;
  }
}

async function attachBuildEvidence(
  root: string,
  head: Snapshot,
  args: ParsedArgs,
  config: ShrikeConfig,
): Promise<BuildEvidenceInfo | null> {
  const explicit =
    stringFlag(args, "build") ?? (config.build.evidence ? config.build.snapshot : undefined);
  const required = booleanFlag(args, "require-build-evidence") || config.build.requireEvidence;
  const loaded = loadBuildEvidence(root, explicit, {
    runBuild: booleanFlag(args, "run-build"),
    buildCommand: stringFlag(args, "build-command"),
  });
  if (loaded.error) {
    throw new CliError(ExitCode.COULD_NOT_RUN, "build-evidence", loaded.error);
  }
  if (loaded.failure && required) {
    throw new CliError(
      ExitCode.COULD_NOT_RUN,
      "build-failed",
      `build evidence is required but the build failed: ${loaded.failure}`,
    );
  }
  if (booleanFlag(args, "no-client-boundary-leaks")) {
    loaded.evidence.clientBoundaryLeaks = false;
  }
  const result = await requestEngine<{ snapshot: Snapshot; join: BuildJoinResult }>("augment-build", {
    snapshot: head,
    build: loaded.evidence,
  });
  if (required && !["full", "partial"].includes(result.join.status)) {
    throw new CliError(
      ExitCode.COULD_NOT_RUN,
      "build-evidence-required",
      `build evidence is required but status is ${result.join.status}${loaded.path ? ` (${loaded.path})` : ""}${loaded.failure ? `: ${loaded.failure}` : ""}`,
    );
  }
  // Build evidence can only add liveness. The engine seeds the evidenced
  // modules and follows known edges so their dependencies are not reported dead.
  if (result.join.status === "full" || result.join.status === "partial") {
    Object.assign(head, result.snapshot);
  }
  return { evidence: loaded.evidence, join: result.join, path: loaded.path };
}

function toReceiptBuild(info: BuildEvidenceInfo | null): ReceiptBuild {
  if (!info) {
    return {
      source: "static",
      snapshotHash: EMPTY_BUILD_HASH,
      version: "unknown",
      status: "absent",
    };
  }
  const status = info.join.status === "none" ? "absent" : info.join.status;
  return {
    source: info.evidence.source || "next analyze export",
    snapshotHash: info.evidence.snapshotHash,
    version: info.evidence.version || "unknown",
    status: status as ReceiptBuild["status"],
  };
}

async function deltaCommand(root: string, args: ParsedArgs): Promise<number> {
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

interface StoredPlan {
  schemaVersion: "1.0";
  createdAt: string;
  baseRef: string | null;
  baseCommit: string;
  headCommit: string;
  headDirty: boolean;
  tier: VerificationTier;
  plan: Plan;
}

function planArtifactPath(root: string, id: string): string {
  return join(root, ".shrike", "plans", `${id}.json`);
}

function storePlan(
  root: string,
  pair: AnalysisPair,
  plan: Plan,
  tier: VerificationTier,
): string {
  const artifact: StoredPlan = {
    schemaVersion: "1.0",
    createdAt: new Date().toISOString(),
    baseRef: pair.baseRef,
    baseCommit: pair.baseCommit,
    headCommit: pair.headCommit,
    headDirty: pair.dirty,
    tier,
    plan,
  };
  mkdirSync(join(root, ".shrike", "plans"), { recursive: true });
  const path = planArtifactPath(root, plan.id);
  writeFileSync(path, `${JSON.stringify(artifact, null, 2)}\n`);
  return path;
}

function readStoredPlan(root: string, id: string): StoredPlan {
  if (!/^plan_[A-Za-z0-9]+$/.test(id)) {
    throw new UsageError("plan id must look like plan_<id>");
  }
  const path = planArtifactPath(root, id);
  if (!existsSync(path)) {
    throw new CliError(
      ExitCode.COULD_NOT_RUN,
      "plan-not-found",
      `plan ${id} was not found at .shrike/plans/${id}.json; run \`shrike plan --base <ref>\` first`,
    );
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(readFileSync(path, "utf8"));
  } catch (error) {
    throw new CliError(
      ExitCode.COULD_NOT_RUN,
      "plan-invalid",
      `plan ${id} could not be parsed: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
  const artifact = parsed as Partial<StoredPlan>;
  if (
    typeof parsed !== "object" || parsed === null ||
    artifact.schemaVersion !== "1.0" ||
    typeof artifact.baseCommit !== "string" ||
    typeof artifact.headCommit !== "string" ||
    typeof artifact.plan !== "object" || artifact.plan === null ||
    !Array.isArray(artifact.plan.batches)
  ) {
    throw new CliError(ExitCode.COULD_NOT_RUN, "plan-invalid", `plan ${id} does not match the stored-plan contract`);
  }
  return artifact as StoredPlan;
}

function baselineConfigHash(checks: CheckSpec[]): string {
  const normalized = checks.map((check) => ({
    name: check.name,
    command: check.command,
    cwd: check.cwd ?? null,
    timeoutMs: check.timeoutMs ?? null,
    reruns: check.reruns ?? null,
    required: check.required ?? true,
  }));
  return `sha256:${createHash("sha256").update(JSON.stringify(normalized)).digest("hex")}`;
}

/**
 * Reuse a baseline only when it belongs to this head commit and this exact
 * check set. Anything else is discarded, never compared against.
 */
async function loadOrCaptureBaseline(
  root: string,
  workingRoot: string,
  head: string,
  checks: CheckSpec[],
  config: ShrikeConfig,
): Promise<Baseline> {
  let path: string;
  try {
    path = resolveInside(root, config.verification.baselinePath);
  } catch (error) {
    throw new ConfigError(
      `verification.baselinePath must stay inside the repository: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
  const configHash = baselineConfigHash(checks);
  if (existsSync(path)) {
    try {
      const parsed = JSON.parse(readFileSync(path, "utf8")) as Baseline;
      if (
        parsed.schemaVersion === "1.0" &&
        parsed.head === head &&
        parsed.configHash === configHash &&
        Array.isArray(parsed.checks)
      ) {
        return parsed;
      }
    } catch {
      // A corrupt baseline is discarded and rebuilt.
    }
  }
  const baseline = await captureBaseline({
    head,
    specs: checks,
    cwd: workingRoot,
    configHash,
  });
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, `${JSON.stringify(baseline, null, 2)}\n`);
  return baseline;
}

async function planCommand(root: string, args: ParsedArgs): Promise<number> {
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

async function pruneCommand(root: string, args: ParsedArgs): Promise<number> {
  rejectUnknownFlags(args, [
    "json", "base", "help", "allow-dirty", "pr", "in-place", "verify", "receipt",
    "dry-run", "no-checks", "build", "require-build-evidence", "run-build",
    "build-command", "no-client-boundary-leaks", "tier",
  ]);
  if (booleanFlag(args, "no-checks")) {
    throw new UsageError("--no-checks is not supported: a verified deletion requires at least one project check");
  }
  const config = await loadConfig(root);
  const allowDirty = booleanFlag(args, "allow-dirty") || config.verification.allowDirty;
  if (isDirty(root) && !allowDirty) throw new UsageError("prune requires a clean working tree");
  const tier = parseTier(stringFlag(args, "tier"), config.verification.tier);
  const pair = await analyzePair(root, args, { allowDirty: config.verification.allowDirty });
  const plan = await requestEngine<Plan>("plan", { snapshot: pair.head, delta: pair.delta, config: engineConfig(config) });
  storePlan(root, pair, plan, tier);
  const receiptBuild = toReceiptBuild(pair.build);
  const make = (
    batch: BatchLike,
    checks: ComparedCheck[],
    outcome: string,
    patchPath: string,
    rejected: ReceiptRejection[] = [],
    recovery?: RecoveryReference,
  ) => makeReceipt(root, pair, plan, batch, checks, outcome, patchPath, rejected, recovery, receiptBuild);
  pair.cleanup();
  if (plan.batches.length === 0) {
    if (booleanFlag(args, "json")) writeJson({ plan, outcome: "clean" });
    else writeHuman("Shrike: no deletions to verify.");
    return ExitCode.OK;
  }
  if (booleanFlag(args, "dry-run")) {
    if (booleanFlag(args, "json")) writeJson(plan);
    else renderPlan(plan);
    return ExitCode.FINDINGS;
  }
  const branch = `${config.pr.branchPrefix}-${plan.id.replace(/^plan_/, "")}`;
  const useWorktree = config.verification.mode === "worktree" && !booleanFlag(args, "in-place");
  if (pair.dirty && useWorktree) {
    throw new UsageError("worktree verification requires a clean tree; use --in-place with --allow-dirty");
  }
  const worktree = useWorktree ? createBranchWorktree(root, branch, pair.headCommit) : null;
  const workingRoot = worktree?.directory ?? root;
  const worktreeDirectory = worktree?.directory ?? null;
  let activeBatch: BatchLike | null = null;
  let activeCompared: ComparedCheck[] = [];
  let activePatch = "";
  let activeRecovery: RecoveryReference | null = null;
  try {
    if (!useWorktree) ensureBranch(root, branch);
    const checks = config.checks.length > 0 ? config.checks : detectRepositoryChecks(workingRoot, { tier });
    if (checks.length === 0) {
      throw new CliError(
        ExitCode.COULD_NOT_RUN,
        "no-checks",
        "no verification checks were found; configure checks or add typecheck/lint/test scripts",
      );
    }
    const baseline = await loadOrCaptureBaseline(root, workingRoot, pair.headCommit, checks, config);
    const checksEnabled = true;
    let outcome: string = "verified";
    let hadRejection = false;
    const receipts: string[] = [];

    const compare = async (): Promise<ComparedCheck[]> => {
      const after = await runChecks(checks, { cwd: workingRoot });
      return after.map((result: CheckResult) => {
        const base = baseline.checks.find((check) => check.name === result.name);
        return { ...result, baselineStatus: base?.status ?? "missing", baselineSignature: base?.signature ?? null, classification: classifyCheck(result, base) };
      });
    };
    const restore = (): void => {
      try {
        restoreAll(workingRoot);
      } catch (error) {
        throw new RevertFailedError(
          `could not restore ${workingRoot}: ${error instanceof Error ? error.message : String(error)}`,
          activeRecovery?.snapshotPath,
        );
      }
      if (process.env.SHRIKE_TEST_FAIL_REVERT === "1") {
        throw new RevertFailedError(
          "forced revert failure (SHRIKE_TEST_FAIL_REVERT=1)",
          activeRecovery?.snapshotPath,
        );
      }
      const dirty = execFileSync("git", ["status", "--porcelain"], { cwd: workingRoot, encoding: "utf8" })
        .split("\n")
        .filter((line) => line.trim().length > 0 && !line.slice(3).startsWith(".shrike/"));
      if (dirty.length > 0) {
        throw new RevertFailedError(
          `working tree is dirty after revert: ${dirty.slice(0, 5).join(", ")}`,
          activeRecovery?.snapshotPath,
        );
      }
    };
    const applyAndCompare = async (symbols: string[]): Promise<{ compared: ComparedCheck[]; blocking: ComparedCheck[]; lines: number }> => {
      restore();
      const lines = applySymbols(workingRoot, pair.head, symbols);
      const compared = checksEnabled && lines > 0 ? await compare() : [];
      return { compared, blocking: blockingChecks(compared), lines };
    };
    const snapshotBatch = (batch: BatchLike): void => {
      activeBatch = batch;
      activeRecovery = writeRecoverySnapshot(root, pair.headCommit, batch.files);
    };

    try {
      for (const batch of plan.batches) {
        snapshotBatch(batch);
        const applied = await applyAndCompare(batch.symbols);
        activeCompared = applied.compared;
        activePatch = writePatch(root, workingRoot, batch.id);
        if (applied.lines === 0) continue;
        if (applied.blocking.length === 0) {
          commitAll(workingRoot, `shrike: remove ${batch.symbols.length} dead symbols (${batch.id})`);
          const receipt = make(batch, applied.compared, "verified", activePatch);
          receipts.push(writeReceipt(root, receipt));
          if (!booleanFlag(args, "quiet") && !booleanFlag(args, "json")) writeHuman(`Verified ${batch.id}: ${batch.symbols.length} symbols, ${batch.files.length} files.`);
          continue;
        }

        // The batch regressed. Bisect by independent units before giving up.
        restore();
        const bisection = await bisectBatch(workingRoot, pair.head, batch, checks, baseline, checksEnabled);
        if (bisection.status === "inconclusive") {
          const receipt = make(batch, bisection.compared.length > 0 ? bisection.compared : applied.compared, "error", activePatch, rejectionsFor(batch.symbols, batch.files, applied.blocking, bisection.reason));
          receipts.push(writeReceipt(root, receipt));
          outcome = "error";
          if (booleanFlag(args, "json")) writeJson({ plan, outcome, receipts });
          else writeHuman(`Shrike: batch ${batch.id} could not be bisected (${bisection.reason}); changes reverted.`);
          return ExitCode.COULD_NOT_RUN;
        }
        if (bisection.status === "pass") {
          // The full batch passed on a rerun: the original failure was flaky.
          const retry = await applyAndCompare(batch.symbols);
          if (retry.blocking.length === 0) {
            activeCompared = retry.compared;
            const retryPatch = writePatch(root, workingRoot, batch.id);
            activePatch = retryPatch;
            commitAll(workingRoot, `shrike: remove ${batch.symbols.length} dead symbols (${batch.id})`);
            const receipt = make(batch, retry.compared, "flaky", retryPatch);
            receipts.push(writeReceipt(root, receipt));
            if (!booleanFlag(args, "quiet") && !booleanFlag(args, "json")) writeHuman(`Verified ${batch.id} (flaky on first attempt).`);
            continue;
          }
          restore();
        }
        if (bisection.status === "interaction-failure" || bisection.rejected.length === 0 || bisection.rejected.length === batch.symbols.length) {
          const receipt = make(batch, applied.compared, bisection.status === "interaction-failure" ? "interaction-failure" : "rejected", activePatch, rejectionsFor(batch.symbols, batch.files, applied.blocking, bisection.reason));
          receipts.push(writeReceipt(root, receipt));
          hadRejection = true;
          outcome = bisection.status === "interaction-failure" ? "interaction-failure" : "rejected";
          if (!booleanFlag(args, "json")) writeHuman(`Shrike: batch ${batch.id} rejected (${bisection.reason}); changes reverted.`);
          continue;
        }

        // Re-run the culprit alone so its rejection carries the exact check and signature.
        const culprit = bisection.rejected;
        const culpritRun = await applyAndCompare(culprit);
        activeCompared = culpritRun.compared;
        const culpritPatch = writePatch(root, workingRoot, `${batch.id}-rejected`);
        activePatch = culpritPatch;
        restore();
        const culpritBatch = subsetBatch(pair.head, batch, culprit, `${batch.id}-rejected`);
        const rejectedReceipt = make(culpritBatch, culpritRun.compared, "rejected", culpritPatch, rejectionsFor(culprit, culpritBatch.files, culpritRun.blocking, bisection.reason));
        receipts.push(writeReceipt(root, rejectedReceipt));
        hadRejection = true;
        outcome = "rejected";
        if (!booleanFlag(args, "json")) writeHuman(`Shrike: rejected ${formatCount(culprit.length, "symbol")} from ${batch.id} (${culpritRun.blocking.map((check) => check.name).join(", ")}).`);

        const remainder = batch.symbols.filter((symbol) => !culprit.includes(symbol));
        if (remainder.length === 0) continue;
        snapshotBatch(batch);
        const remainderRun = await applyAndCompare(remainder);
        activeCompared = remainderRun.compared;
        if (remainderRun.blocking.length > 0) {
          restore();
          const wholeReceipt = make(batch, remainderRun.compared, "rejected", activePatch, rejectionsFor(batch.symbols, batch.files, remainderRun.blocking, "remainder failed after bisection"));
          receipts.push(writeReceipt(root, wholeReceipt));
          continue;
        }
        const remainderBatch = subsetBatch(pair.head, batch, remainder, `${batch.id}-verified`);
        const remainderPatch = writePatch(root, workingRoot, remainderBatch.id);
        activePatch = remainderPatch;
        commitAll(workingRoot, `shrike: remove ${remainder.length} dead symbols (${batch.id})`);
        const verifiedReceipt = make(remainderBatch, remainderRun.compared, "verified", remainderPatch);
        receipts.push(writeReceipt(root, verifiedReceipt));
        if (!booleanFlag(args, "quiet") && !booleanFlag(args, "json")) writeHuman(`Verified ${remainder.length} of ${batch.symbols.length} symbols from ${batch.id}.`);
      }
    } catch (error) {
      if (!(error instanceof RevertFailedError)) throw error;
      const recovery = activeRecovery ?? {
        snapshotPath: "",
        restoreCommand: "inspect the worktree and restore it from git",
      };
      let receiptPath: string | null = null;
      if (activeBatch) {
        try {
          const receipt = make(activeBatch, activeCompared, "revert-failed", activePatch, [], recovery);
          receiptPath = writeReceipt(root, receipt);
          receipts.push(receiptPath);
        } catch (receiptError) {
          writeStderr(`Could not write revert-failed receipt: ${receiptError instanceof Error ? receiptError.message : String(receiptError)}`);
        }
      }
      const message = `revert failed; recovery snapshot: ${recovery.snapshotPath || "unavailable"}`;
      if (booleanFlag(args, "json")) {
        writeJson({ plan, outcome: "revert-failed", receipts, branch, worktree: worktreeDirectory, recovery, error: message });
      } else {
        writeStderr(`Shrike: ${message}`);
        if (recovery.snapshotPath) writeStderr(`Restore with: ${recovery.restoreCommand}`);
      }
      return ExitCode.REVERT_FAILED;
    }
    if (booleanFlag(args, "pr")) openPullRequest(root, branch, plan, receipts, config);
    if (booleanFlag(args, "json")) writeJson({ plan, outcome, receipts, branch, worktree: worktreeDirectory });
    else {
      writeHuman(`Shrike: ${formatCount(plan.totals.symbols, "symbol")} removed on ${branch}.`);
      if (worktreeDirectory) writeHuman(`Worktree: ${worktreeDirectory}`);
      if (!booleanFlag(args, "pr")) writeHuman(`Open a draft PR with: gh pr create --draft --head ${branch}`);
    }
    return hadRejection ? ExitCode.VERIFICATION_FAILED : ExitCode.OK;
  } finally {
    worktree?.cleanup();
  }
}

interface BatchLike {
  id: string;
  symbols: string[];
  files: string[];
  lines: number;
}

function makeReceipt(
  root: string,
  pair: AnalysisPair,
  plan: Plan,
  batch: BatchLike,
  checks: ComparedCheck[],
  outcome: string,
  patchPath: string,
  rejected: ReceiptRejection[] = [],
  recovery?: RecoveryReference,
  build?: ReceiptBuild,
) {
  const policy = plan.policy;
  return buildReceipt({
    tool: { name: "shrike", version: VERSION },
    base: { ref: pair.baseRef, commit: pair.baseCommit, graphHash: pair.base.graphHash },
    head: { commit: pair.headCommit, graphHash: pair.head.graphHash, dirty: pair.dirty },
    plan: { id: plan.id, hash: plan.deltaHash },
    batch: { id: batch.id, symbols: batch.symbols, files: batch.files, linesRemoved: batch.lines, patches: [patchPath || `.shrike/patches/${batch.id}.patch`] },
    checks: toReceiptChecks(checks, { root }),
    outcome: outcome as never,
    rejected,
    policy: {
      entryPolicy: (policy?.entryPolicy as "local" | "production" | undefined) ?? "local",
      maxSymbols: policy?.maxSymbols ?? 50,
      maxFiles: policy?.maxFiles ?? 10,
      allowOversized: policy?.allowOversized ?? false,
    },
    environment: captureEnvironment({ packageManager: detectPackageManager(root) }),
    ...(recovery ? { recovery } : {}),
    ...(build ? { build } : {}),
  });
}

function blockingChecks(checks: ComparedCheck[]): ComparedCheck[] {
  return checks.filter((check) => check.required && ["new-failure", "fail", "timeout", "error", "flaky"].includes(check.classification ?? ""));
}

function rejectionsFor(symbols: string[], files: string[], blocking: ComparedCheck[], reason: string): ReceiptRejection[] {
  return blocking.map((check) => ({
    symbols,
    files,
    check: check.name,
    signature: check.signature ?? `status:${check.status}`,
    reason,
  }));
}

function subsetBatch(snapshot: Snapshot, batch: BatchLike, symbols: string[], id: string): BatchLike {
  const files = [...new Set(symbols
    .map((symbol) => snapshot.symbols.find((candidate) => candidate.key === symbol)?.file)
    .filter((file): file is string => Boolean(file)))];
  const lines = symbols.reduce((total, symbol) => total + (snapshot.symbols.find((candidate) => candidate.key === symbol)?.lines ?? 1), 0);
  return { id, symbols, files: files.length > 0 ? files : batch.files, lines };
}

async function bisectBatch(
  workingRoot: string,
  snapshot: Snapshot,
  batch: BatchLike,
  checks: ShrikeConfig["checks"],
  baseline: Baseline,
  checksEnabled: boolean,
): Promise<{ status: string; rejected: string[]; compared: ComparedCheck[]; reason: string }> {
  const units: BisectUnit[] = batch.symbols
    .map((symbol) => ({ id: symbol, files: [snapshot.symbols.find((candidate) => candidate.key === symbol)?.file ?? ""] }))
    .filter((unit) => unit.files[0] !== "");
  let lastCompared: ComparedCheck[] = [];
  let lastReason = "";
  const result = await bisectFailure(units, async (subset) => {
    restoreAll(workingRoot);
    try {
      const lines = applySymbols(workingRoot, snapshot, subset.map((unit) => unit.id));
      if (lines === 0 || !checksEnabled) return true;
      const after = await runChecks(checks, { cwd: workingRoot });
      const compared: ComparedCheck[] = after.map((check: CheckResult) => {
        const base = baseline.checks.find((candidate) => candidate.name === check.name);
        return { ...check, baselineStatus: base?.status ?? "missing", baselineSignature: base?.signature ?? null, classification: classifyCheck(check, base) };
      });
      lastCompared = compared;
      const blocking = blockingChecks(compared);
      if (blocking.length > 0) lastReason = blocking.map((check) => check.name).join(", ");
      return blocking.length === 0;
    } finally {
      restoreAll(workingRoot);
    }
  }, { maxAttempts: 16 });
  return { status: result.status, rejected: result.rejected.map((unit) => unit.id), compared: lastCompared, reason: result.reason || lastReason || "check regression" };
}

function applySymbols(root: string, snapshot: Snapshot, symbols: string[]): number {
  const byFile = new Map<string, string[]>();
  for (const key of symbols) {
    const file = snapshot.symbols.find((candidate) => candidate.key === key)?.file;
    if (!file) continue;
    const list = byFile.get(file);
    if (list) list.push(key);
    else byFile.set(file, [key]);
  }
  // A file whose every symbol is being deleted is itself dead. Removing the
  // whole file is what prevents dangling imports from a deleted dependency
  // (for example `b.ts` importing `a.ts` when both are removed together).
  const symbolsByFile = new Map<string, string[]>();
  for (const symbol of snapshot.symbols) {
    const list = symbolsByFile.get(symbol.file);
    if (list) list.push(symbol.key);
    else symbolsByFile.set(symbol.file, [symbol.key]);
  }
  const live = new Set(snapshot.live ?? []);
  let removed = 0;
  for (const [file, keys] of [...byFile.entries()].sort(([left], [right]) => left.localeCompare(right))) {
    let path: string;
    try {
      path = resolveInside(root, file);
    } catch (error) {
      throw new CliError(
        ExitCode.COULD_NOT_RUN,
        "invalid-path",
        `refusing to edit a path outside the repository: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
    if (!existsSync(path)) continue;
    const content = readFileSync(path, "utf8");
    const lines = content.split("\n");
    const deleting = new Set(keys);
    const fileSymbols = (symbolsByFile.get(file) ?? []).filter(
      (key) => !key.endsWith("#__module__"),
    );
    if (
      fileSymbols.length > 0 &&
      fileSymbols.every((key) => deleting.has(key)) &&
      !live.has(`${file}#__module__`)
    ) {
      removed += contentLineCount(content);
      rmSync(path);
      continue;
    }
    const ranges: [number, number][] = [];
    for (const key of keys) {
      const symbol = snapshot.symbols.find((candidate) => candidate.key === key && candidate.file === file);
      if (!symbol) continue;
      const startLine = symbol.span.line;
      const endLine = Math.min(lines.length, lineAt(content, symbol.span.end));
      if (!Number.isInteger(startLine) || startLine < 1 || startLine > lines.length) {
        throw new CliError(ExitCode.COULD_NOT_RUN, "invalid-span", `invalid span for ${key}`);
      }
      ranges.push([startLine - 1, Math.max(startLine - 1, endLine - 1)]);
    }
    if (ranges.length === 0) continue;
    const remove = new Set<number>();
    for (const [start, end] of ranges) for (let line = start; line <= end; line += 1) remove.add(line);
    const kept = lines.filter((_line, index) => !remove.has(index));
    removed += lines.length - kept.length;
    writeFileSync(path, kept.join("\n"));
  }
  return removed;
}

function contentLineCount(content: string): number {
  if (content.length === 0) return 0;
  const lines = content.split("\n");
  return content.endsWith("\n") ? lines.length - 1 : lines.length;
}

function lineAt(content: string, offset: number): number {
  let line = 1;
  for (let index = 0; index < Math.min(offset, content.length); index += 1) if (content[index] === "\n") line += 1;
  return line;
}

function writePatch(root: string, workingRoot: string, batchId: string): string {
  mkdirSync(join(root, ".shrike", "patches"), { recursive: true });
  const safeId = batchId.replace(/[^A-Za-z0-9_-]/g, "_");
  const relative = `.shrike/patches/${safeId}.patch`;
  writeFileSync(join(root, relative), execGitDiff(workingRoot));
  return relative;
}

function execGitDiff(root: string): string {
  try {
    return execFileSync("git", ["diff"], { cwd: root, encoding: "utf8" });
  } catch { return ""; }
}

function openPullRequest(
  root: string,
  branch: string,
  plan: Plan,
  receiptPaths: string[],
  config: ShrikeConfig,
): void {
  const base = config.pr.base ?? plan.base.ref ?? "main";
  const receipts = receiptPaths.map((path) => readReceipt(path));
  const batches = receipts.map((receipt) => ({
    id: receipt.batch.id,
    symbols: receipt.batch.symbols.length,
    files: receipt.batch.files.length,
    lines: receipt.batch.linesRemoved,
    outcome: receipt.outcome,
    checks: receipt.checks.map((check) => `${check.name}:${check.classification ?? check.status}`).join(", "),
  }));
  const rejected = receipts.flatMap((receipt) => receipt.rejected.map((item) => ({
    symbols: item.symbols,
    check: item.check ?? "unknown",
    reason: item.reason ?? "regression",
  })));
  const body = renderPrBody({
    planId: plan.id,
    branch,
    baseRef: base,
    headCommit: plan.head.commit,
    orphanDelta: plan.totals.symbols,
    batches,
    rejected,
    receipts,
  });
  const title = config.pr.title === "Shrike: verified cleanup"
    ? `Shrike: remove ${plan.totals.symbols} dead symbols`
    : config.pr.title;
  const remote = spawnSync("git", ["rev-parse", "--verify", `origin/${branch}`], { cwd: root, encoding: "utf8" });
  if (remote.status === 0) {
    const ancestor = spawnSync("git", ["merge-base", "--is-ancestor", `origin/${branch}`, "HEAD"], { cwd: root, encoding: "utf8" });
    if (ancestor.status !== 0) {
      writeStderr(`Remote branch ${branch} diverged; refusing to force-push. Push manually after review.`);
      return;
    }
  }
  const draft = config.pr.draft ? ["--draft"] : [];
  const push = spawnSync("git", ["push", "-u", "origin", branch], { cwd: root, encoding: "utf8" });
  if (push.status !== 0) {
    writeStderr(`Could not push ${branch}; open the PR manually: gh pr create ${draft.join(" ")} --head ${branch} --base ${base}`);
    return;
  }
  const gh = spawnSync(
    "gh",
    ["pr", "create", ...draft, "--head", branch, "--base", base, "--title", title, "--body", body],
    { cwd: root, encoding: "utf8" },
  );
  if (gh.status !== 0) {
    writeStderr(`gh unavailable or failed; open the PR manually: gh pr create ${draft.join(" ")} --head ${branch} --base ${base}`);
    return;
  }
  const url = gh.stdout.trim();
  if (config.pr.labels.length > 0) {
    for (const label of config.pr.labels) {
      const result = spawnSync("gh", ["pr", "edit", url, "--add-label", label], { cwd: root, encoding: "utf8" });
      if (result.status !== 0) writeStderr(`Could not add label ${label} to ${url}.`);
    }
  }
  writeHuman(url);
  if (!config.pr.commentReceipt) return;
  for (const receipt of receipts) {
    const comment = spawnSync("gh", ["pr", "comment", url, "--body", renderReceiptComment(receipt)], { cwd: root, encoding: "utf8" });
    if (comment.status !== 0) writeStderr(`Could not post receipt ${receipt.id} to the PR.`);
  }
}

function parseTier(value: string | undefined, fallback: VerificationTier): VerificationTier {
  if (value === undefined) return fallback;
  if (value === "fast" || value === "standard" || value === "full") return value;
  throw new UsageError(`invalid check tier: ${value}; expected fast, standard, or full`);
}

async function verifyCommand(root: string, args: ParsedArgs): Promise<number> {
  const id = stringFlag(args, "plan") ?? args.positionals[0];
  if (!id) throw new UsageError("verify requires --plan <id> or a receipt id");
  // Receipt IDs remain accepted for `shrike verify <rcpt_...>` so an existing
  // receipt can be inspected after the fact. A plan ID re-runs verification.
  if (id.startsWith("rcpt_")) {
    const receipt = readReceipt(join(root, ".shrike", "receipts", `${id}.json`));
    if (booleanFlag(args, "json")) writeJson(receipt);
    else writeHuman(`Receipt ${receipt.id}: ${receipt.outcome}`);
    return receipt.outcome === "verified" ? ExitCode.OK : ExitCode.VERIFICATION_FAILED;
  }
  rejectUnknownFlags(args, ["plan", "json", "help", "in-place", "allow-dirty", "quiet", "tier"]);
  const artifact = readStoredPlan(root, id);
  const config = await loadConfig(root);
  const allowDirty = booleanFlag(args, "allow-dirty") || config.verification.allowDirty;
  if (isDirty(root) && !allowDirty) {
    throw new UsageError("verify requires a clean working tree");
  }
  if (artifact.headDirty) {
    throw new CliError(
      ExitCode.COULD_NOT_RUN,
      "dirty-plan",
      `plan ${artifact.plan.id} was computed on a dirty tree; commit the changes and re-plan before verifying`,
    );
  }
  const current = headCommit(root);
  if (current !== artifact.headCommit) {
    throw new CliError(
      ExitCode.COULD_NOT_RUN,
      "plan-head-mismatch",
      `plan ${artifact.plan.id} targets ${artifact.headCommit.slice(0, 12)}, but HEAD is ${current.slice(0, 12)}`,
    );
  }
  const tier = parseTier(stringFlag(args, "tier"), artifact.tier ?? config.verification.tier);
  const pairArgs: ParsedArgs = {
    positionals: [],
    values: new Map([["base", artifact.baseRef ?? artifact.baseCommit]]),
  };
  const pair = await analyzePair(root, pairArgs, { allowDirty: config.verification.allowDirty });
  const receipts: string[] = [];
  try {
    if (pair.baseCommit !== artifact.baseCommit) {
      throw new CliError(
        ExitCode.COULD_NOT_RUN,
        "plan-base-mismatch",
        `plan ${artifact.plan.id} targets base ${artifact.baseCommit.slice(0, 12)}, but the merge base is ${pair.baseCommit.slice(0, 12)}`,
      );
    }
    const checks = config.checks.length > 0
      ? config.checks
      : detectRepositoryChecks(root, { tier });
    if (checks.length === 0) {
      throw new CliError(
        ExitCode.COULD_NOT_RUN,
        "no-checks",
        "no verification checks were found; configure checks or add typecheck/lint/test scripts",
      );
    }
    const worktree = createWorktree(root, artifact.headCommit, "verify");
    try {
      const baseline = await loadOrCaptureBaseline(
        root,
        worktree.directory,
        artifact.headCommit,
        checks,
        config,
      );
      const compare = async (): Promise<ComparedCheck[]> => {
        const after = await runChecks(checks, { cwd: worktree.directory });
        return after.map((result: CheckResult) => {
          const base = baseline.checks.find((check) => check.name === result.name);
          return {
            ...result,
            baselineStatus: base?.status ?? "missing",
            baselineSignature: base?.signature ?? null,
            classification: classifyCheck(result, base),
          };
        });
      };
      const receiptBuild = toReceiptBuild(pair.build);
      const make = (
        batch: BatchLike,
        compared: ComparedCheck[],
        outcome: string,
        patchPath: string,
        rejected: ReceiptRejection[] = [],
      ) => makeReceipt(root, pair, artifact.plan, batch, compared, outcome, patchPath, rejected, undefined, receiptBuild);
      let outcome = "verified";
      let hadRejection = false;
      for (const batch of artifact.plan.batches) {
        restoreAll(worktree.directory);
        const lines = applySymbols(worktree.directory, pair.head, batch.symbols);
        if (lines === 0) continue;
        const patchPath = writePatch(root, worktree.directory, batch.id);
        const compared = await compare();
        const blocking = blockingChecks(compared);
        if (blocking.length > 0) {
          restoreAll(worktree.directory);
          const receipt = make(batch, compared, "rejected", patchPath, rejectionsFor(batch.symbols, batch.files, blocking, "re-verification regression"));
          receipts.push(writeReceipt(root, receipt));
          outcome = "rejected";
          hadRejection = true;
          if (!booleanFlag(args, "json") && !booleanFlag(args, "quiet")) {
            writeHuman(`Rejected ${batch.id}: ${blocking.map((check) => check.name).join(", ")} failed; changes reverted.`);
          }
          continue;
        }
        commitAll(worktree.directory, `shrike: verify ${batch.id}`);
        const receipt = make(batch, compared, "verified", patchPath);
        receipts.push(writeReceipt(root, receipt));
        if (!booleanFlag(args, "json") && !booleanFlag(args, "quiet")) {
          writeHuman(`Verified ${batch.id}: ${batch.symbols.length} symbols, ${batch.files.length} files.`);
        }
      }
      if (booleanFlag(args, "json")) {
        writeJson({ plan: artifact.plan, outcome, receipts });
      } else {
        writeHuman(`Shrike: re-verification ${outcome}; ${formatCount(receipts.length, "receipt")}.`);
      }
      return hadRejection ? ExitCode.VERIFICATION_FAILED : ExitCode.OK;
    } finally {
      worktree.cleanup();
    }
  } finally {
    pair.cleanup();
  }
}

function receiptCommand(root: string, args: ParsedArgs): number {
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

function recoverCommand(root: string, args: ParsedArgs): number {
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

async function proofCommand(root: string, args: ParsedArgs): Promise<number> {
  const symbol = args.positionals[0];
  if (!symbol) throw new UsageError("proof requires a symbol key (path#Name)");
  const config = await loadConfig(root);
  const snapshot = await analyze(root, config);
  const proof = await requestEngine("proof", { snapshot, symbol });
  if (booleanFlag(args, "json")) writeJson(proof);
  else writeHuman(JSON.stringify(proof, null, 2));
  return ExitCode.OK;
}

async function evidenceCommand(root: string, args: ParsedArgs): Promise<number> {
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

const HOOK_TOTAL_BUDGET_MS = 60_000;
const HOOK_CHECKS_BUDGET_MS = 40_000;

function environmentMs(name: string, fallback: number): number {
  const raw = process.env[name];
  if (raw === undefined || raw === "") return fallback;
  const value = Number(raw);
  return Number.isFinite(value) && value >= 0 ? value : fallback;
}

/**
 * Read a baseline for advisory comparison. The hook never captures one: the
 * prune flow owns baseline capture, and the hook must stay inside its budget.
 *
 * A stored baseline may cover the standard or full tier. Reuse the entries that
 * match the hook's fast check set by name and command, so a baseline captured
 * by `prune` can still explain a pre-existing failure to the hook.
 */
function hookBaseline(
  root: string,
  config: ShrikeConfig,
  head: string,
  checks: CheckSpec[],
): Baseline | null {
  let path: string;
  try {
    path = resolveInside(root, config.verification.baselinePath);
  } catch {
    return null;
  }
  if (!existsSync(path)) return null;
  try {
    const parsed = JSON.parse(readFileSync(path, "utf8")) as Baseline;
    if (
      parsed.schemaVersion !== "1.0" ||
      parsed.head !== head ||
      !Array.isArray(parsed.checks)
    ) return null;
    const byName = new Map(parsed.checks.map((check) => [check.name, check]));
    const matched: CheckResult[] = [];
    for (const spec of checks) {
      const stored = byName.get(spec.name);
      if (!stored || stored.command !== spec.command) return null;
      matched.push(stored);
    }
    return { ...parsed, checks: matched };
  } catch {
    // A corrupt baseline is advisory-only here; the next prune rebuilds it.
  }
  return null;
}

function hookCheckSummary(results: ComparedCheck[], baseline: Baseline | null): string {
  if (results.length === 0) return "skipped";
  if (results.some((result) => result.status === "timeout")) return "timeout";
  if (results.some((result) => result.status === "error")) return "error";
  if (baseline) {
    const blocking = new Set(["new-failure", "fail", "timeout", "error", "flaky"]);
    return results.some((result) => blocking.has(result.classification)) ? "fail" : "pass";
  }
  return results.some((result) => result.required && result.status !== "pass") ? "fail" : "pass";
}

/** Run fast checks sequentially against one deadline. */
async function runHookChecks(
  root: string,
  specs: CheckSpec[],
  deadline: number,
  baseline: Baseline | null,
): Promise<{ status: string; results: ComparedCheck[] }> {
  const results: ComparedCheck[] = [];
  for (const spec of specs) {
    const remaining = deadline - Date.now();
    if (remaining < 250) return { status: "skipped-budget", results };
    const budgetLimited = remaining <= (spec.timeoutMs ?? Number.POSITIVE_INFINITY);
    const result = await runCheckWithReruns(
      { ...spec, timeoutMs: Math.min(spec.timeoutMs ?? remaining, remaining), reruns: 0 },
      { cwd: root },
    );
    const base = baseline?.checks.find((check) => check.name === result.name);
    const compared: ComparedCheck = {
      ...result,
      baselineStatus: base?.status ?? "missing",
      baselineSignature: base?.signature ?? null,
      classification: classifyCheck(result, base),
    };
    results.push(compared);
    const blocking = baseline
      ? ["new-failure", "fail", "timeout", "error", "flaky"].includes(compared.classification)
      : compared.required && compared.status !== "pass";
    if (blocking) {
      if (compared.status === "timeout" && budgetLimited) return { status: "skipped-budget", results };
      if (compared.status === "timeout") return { status: "timeout", results };
      if (compared.status === "error") return { status: "error", results };
      return { status: "fail", results };
    }
  }
  return { status: hookCheckSummary(results, baseline), results };
}

async function hookCommand(root: string, args: ParsedArgs): Promise<number> {
  const action = args.positionals[0];
  if (action === "run") return hookRunCommand(root, args);
  if (action !== "install" && action !== "uninstall") {
    throw new UsageError("usage: shrike hook install|uninstall --claude|--cursor|--opencode");
  }
  const target: HookTarget | null = booleanFlag(args, "claude") ? "claude" : booleanFlag(args, "cursor") ? "cursor" : booleanFlag(args, "opencode") ? "opencode" : null;
  if (!target) throw new UsageError("choose --claude, --cursor, or --opencode");
  const result = action === "install" ? installHook(root, target) : uninstallHook(root, target);
  if (booleanFlag(args, "json")) writeJson(result);
  else writeHuman(`${action === "install" ? "Installed" : "Removed"} ${target} hook ${result.changed ? `in ${result.path}` : `(no change: ${result.path})`}`);
  return ExitCode.OK;
}

async function hookRunCommand(root: string, args: ParsedArgs): Promise<number> {
  const quiet = booleanFlag(args, "quiet");
  const startedAt = Date.now();
  const totalBudgetMs = Math.max(
    1,
    Math.min(HOOK_TOTAL_BUDGET_MS, environmentMs("SHRIKE_HOOK_TOTAL_BUDGET_MS", HOOK_TOTAL_BUDGET_MS)),
  );
  let base: string;
  try {
    base = process.env.SHRIKE_HOOK_BASE ?? resolveDefaultBase(root);
  } catch {
    if (!quiet) writeHuman("Shrike hook: no base branch; skipping.");
    return ExitCode.OK;
  }
  const cli = join(import.meta.dirname, "main.ts");
  const result = spawnSync(
    process.execPath,
    [cli, "delta", "--json", "--allow-dirty", "--base", base],
    {
      cwd: root,
      encoding: "utf8",
      timeout: Math.max(1, Math.min(55_000, totalBudgetMs)),
      env: { ...process.env, SHRIKE_HOOK: "1" },
    },
  );
  if (result.error || !result.stdout.trim()) {
    if (!quiet) writeHuman("Shrike hook: delta unavailable; skipping.");
    return ExitCode.OK;
  }
  let delta: unknown;
  try {
    delta = JSON.parse(result.stdout);
  } catch {
    if (!quiet) writeHuman("Shrike hook: delta did not produce JSON; skipping.");
    return ExitCode.OK;
  }

  let head = "HEAD";
  try { head = headCommit(root); } catch { /* the brief still records the delta */ }
  let changedFiles: string[] = [];
  try {
    const comparison = mergeBase(root, base);
    changedFiles = git(root, ["diff", "--name-only", comparison], true)
      .split("\n")
      .filter((line) => line.trim().length > 0);
  } catch {
    changedFiles = git(root, ["diff", "--name-only", base], true)
      .split("\n")
      .filter((line) => line.trim().length > 0);
  }

  // Fast checks are advisory. Config, discovery, or check failures must never
  // block the agent, and the whole hook must stay inside its 60-second budget.
  let checksStatus = "skipped";
  try {
    const config = await loadConfig(root);
    const specs: CheckSpec[] = config.checks.length > 0
      ? config.checks.filter((check) => (check.tier ?? "standard") === "fast")
      : detectRepositoryChecks(root, { tier: "fast" });
    if (specs.length > 0) {
      const checksBudgetMs = Math.min(
        environmentMs("SHRIKE_HOOK_CHECKS_BUDGET_MS", HOOK_CHECKS_BUDGET_MS),
        totalBudgetMs,
      );
      const deadline = Math.min(startedAt + totalBudgetMs, Date.now() + checksBudgetMs);
      const baseline = hookBaseline(root, config, head, specs);
      checksStatus = (await runHookChecks(root, specs, deadline, baseline)).status;
    }
  } catch {
    checksStatus = "skipped";
  }

  const brief = buildCleanupBrief({
    repo: root,
    base,
    head,
    dirty: isDirty(root),
    delta,
    checks: checksStatus,
    changedFiles,
  });
  try {
    mkdirSync(join(root, ".shrike"), { recursive: true });
    writeFileSync(join(root, ".shrike", "brief.json"), `${JSON.stringify(brief, null, 2)}\n`);
  } catch {
    if (!quiet) writeHuman("Shrike hook: could not write the brief; skipping.");
    return ExitCode.OK;
  }
  if (booleanFlag(args, "json")) {
    writeJson(brief);
  } else if (!quiet) {
    writeHuman(`Shrike: ${formatCount(brief.newlyDead, "newly dead symbol")} (${brief.files} files, ${brief.untouchedFiles} untouched).`);
    for (const item of brief.top.slice(0, 3)) writeHuman(`  ${item.symbol}  [${item.file}]`);
    writeHuman(`Next: ${brief.nextCommand}`);
  }
  return ExitCode.OK;
}

const MCP_MAX_OUTPUT = 256 * 1024;

function validateMcpArgument(name: string, value: string): void {
  if (name === "base") {
    if (
      value.startsWith("-") ||
      value.includes("..") ||
      !/^[A-Za-z0-9._/@{}^~:-]+$/.test(value)
    ) {
      throw new UsageError(`invalid base ref: ${value}`);
    }
    return;
  }
  if (name === "symbol") {
    if (!/^[^#]+#[^#]+$/.test(value)) throw new UsageError("symbol must look like path#Name");
    safeRelativePath(value.slice(0, value.indexOf("#")));
    return;
  }
  if (name === "id" && !/^rcpt_[A-Za-z0-9]{8,64}$/.test(value)) {
    throw new UsageError("receipt id must look like rcpt_<hex>");
  }
}

async function mcpCommand(root: string, args: ParsedArgs): Promise<number> {
  if (args.positionals[0] !== "serve") throw new UsageError("usage: shrike mcp serve");
  const cli = join(import.meta.dirname, "main.ts");
  const rl = createInterface({ input: process.stdin, crlfDelay: Infinity });
  for await (const line of rl) {
    if (!line.trim()) continue;
    let request: { id?: unknown; method?: string; params?: { name?: string; arguments?: unknown } };
    try { request = JSON.parse(line); } catch { continue; }
    const respond = (result: unknown) => process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: request.id ?? null, result })}\n`);
    const fail = (code: number, message: string) =>
      process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: request.id ?? null, error: { code, message } })}\n`);
    if (request.method === "initialize") {
      respond({ protocolVersion: "2024-11-05", capabilities: { tools: {} }, serverInfo: { name: "shrike", version: VERSION } });
    } else if (request.method === "tools/list") {
      respond({ tools: [
        { name: "shrike_delta", description: "Whole-graph PR orphan delta", inputSchema: { type: "object", properties: { base: { type: "string" } } } },
        { name: "shrike_plan", description: "Verified deletion batches", inputSchema: { type: "object", properties: { base: { type: "string" } } } },
        { name: "shrike_proof", description: "Witness or proof trace", inputSchema: { type: "object", properties: { symbol: { type: "string" } }, required: ["symbol"] } },
        { name: "shrike_receipt", description: "Show or list verified deletion receipts", inputSchema: { type: "object", properties: { id: { type: "string" } } } },
      ] });
    } else if (request.method === "tools/call") {
      const name = request.params?.name ?? "";
      const command = name.replace(/^shrike_/, "");
      if (!["delta", "plan", "proof", "receipt"].includes(command)) {
        fail(-32601, "unknown tool");
        continue;
      }
      const toolArgs = (request.params?.arguments ?? {}) as Record<string, unknown>;
      try {
        const argv = [process.execPath, cli, command, "--json"];
        if (typeof toolArgs.base === "string") {
          validateMcpArgument("base", toolArgs.base);
          argv.push("--base", toolArgs.base);
        }
        if (typeof toolArgs.symbol === "string") {
          validateMcpArgument("symbol", toolArgs.symbol);
          argv.push(toolArgs.symbol);
        }
        if (typeof toolArgs.id === "string") {
          validateMcpArgument("id", toolArgs.id);
          argv.push(toolArgs.id);
        }
        const output = await runCli(root, argv);
        const text = output.stdout.length > MCP_MAX_OUTPUT
          ? `${output.stdout.slice(0, MCP_MAX_OUTPUT)}\n[truncated at ${MCP_MAX_OUTPUT} bytes]`
          : output.stdout;
        respond({ content: [{ type: "text", text }], isError: output.code !== 0 });
      } catch (error) {
        fail(-32602, error instanceof Error ? error.message : String(error));
      }
    } else if (request.id !== undefined) {
      fail(-32601, "method not found");
    }
  }
  return ExitCode.OK;
}

async function runCli(root: string, argv: string[]): Promise<{ code: number; stdout: string }> {
  const { spawn } = await import("node:child_process");
  return new Promise((resolveRun) => {
    const child = spawn(argv[0]!, argv.slice(1), { cwd: root, stdio: ["ignore", "pipe", "inherit"] });
    let stdout = "";
    child.stdout.setEncoding("utf8");
    child.stdout.on("data", (chunk: string) => { stdout += chunk; });
    child.on("close", (code) => resolveRun({ code: code ?? 4, stdout }));
  });
}

async function cacheCommand(root: string, args: ParsedArgs): Promise<number> {
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

function renderDelta(delta: DeltaReport): void {
  if (delta.incomparable) {
    writeHuman("Shrike: snapshots are incomparable; refusing to report a delta.");
    writeHuman(JSON.stringify(delta.incomparable, null, 2));
    return;
  }
  writeHuman(`Shrike delta: ${formatCount(delta.newlyDead.length, "newly dead symbol")} in ${formatCount(delta.newlyDeadFiles.length, "file")}`);
  for (const item of delta.newlyDead) {
    writeHuman(`  ${item.symbol}  -${item.estimatedLines} lines  [${item.file}]`);
  }
  for (const file of delta.newlyDeadFiles) writeHuman(`  ${file}  (file)`);
  writeHuman(`Cascade: ${delta.cascade.symbols} symbols, ${delta.cascade.files} files, ${delta.cascade.lines} lines`);
}

function renderPlan(plan: Plan): void {
  writeHuman(`Plan ${plan.id}: ${plan.totals.symbols} symbols, ${plan.totals.files} files, ${plan.totals.lines} lines`);
  for (const batch of plan.batches) {
    writeHuman(`  ${batch.id}: ${batch.symbols.length} symbols, ${batch.files.length} files, ${batch.lines} lines`);
  }
  if (plan.blocked.length > 0) writeHuman(`Blocked: ${formatCount(plan.blocked.length, "item")}`);
}

function printHelp(): void {
  writeHuman(`shrike ${VERSION} — verified dead-code cleanup

Usage: shrike <command> [options]

Commands:
  init                 Create shrike.config.ts and ignore .shrike/
  scan                 Current-state dead-code evidence
  delta --base <ref>   Whole-graph PR orphan delta
  plan --base <ref>    Deletion batches and blast radius
  prune --base <ref> [--build <path>]  Apply, verify, commit batches (branch only)
  verify --plan <id>   Re-check a plan/receipt
  receipt [id]         Show receipts
  proof <symbol>       Witness path or proof trace
  evidence [--build <path>] [--require-build-evidence]  Join build-graph evidence
  hook install --claude|--cursor|--opencode
  mcp serve            MCP server over stdio
  cache clean|verify   Cache maintenance
  version              Version and protocol

All read-only commands support --json. Exit codes: 0 clean, 1 findings,
2 usage, 3 verification failed, 4 could not run, 5 revert failed.`);
}

main(process.argv.slice(2))
  // Set the code instead of calling process.exit(): exiting immediately can
  // truncate large JSON documents still draining to a pipe.
  .then((code) => {
    process.exitCode = code;
  })
  .catch((error: unknown) => {
    const cliError = isCliError(error) ? error : new CliError(ExitCode.COULD_NOT_RUN, "internal", error instanceof Error ? error.message : String(error));
    writeStderr(`shrike: ${cliError.message}`);
    if (process.argv.includes("--json")) writeJsonError({ code: cliError.code, message: cliError.message, exitCode: cliError.exitCode, details: cliError.details });
    process.exitCode = cliError.exitCode;
  });
