#!/usr/bin/env node
import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { parseArgs, booleanFlag, stringFlag, rejectUnknownFlags, type ParsedArgs } from "./args.ts";
import { loadConfig, engineConfig, findConfigPath, repoRoot, type ShrikeConfig } from "./config.ts";
import { requestEngine } from "./engine.ts";
import { ExitCode, isCliError, UsageError, ConfigError, CliError } from "./errors.ts";
import { writeJson, writeHuman, writeStderr, writeJsonError, formatCount } from "./output.ts";
import {
  createBranchWorktree, createWorktree, ensureBranch, commitAll, headCommit, isDirty, mergeBase,
  resolveDefaultBase, restoreAll,
} from "./git.ts";
import { VERSION, PROTOCOL_VERSION } from "./version.ts";
import {
  captureBaseline, classifyCheck, detectRepositoryChecks, runChecks,
  type Baseline, type CheckResult, type ComparedCheck,
} from "@shrike/verifier";
import {
  buildReceipt, writeReceipt, readReceipt, listReceipts, captureEnvironment, toReceiptChecks,
} from "@shrike/receipts";
import { installHook, type HookTarget } from "@shrike/agent";

interface Snapshot {
  files: { path: string; contentHash: string; lines: number }[];
  symbols: { key: string; file: string; kind: string; span: { start: number; end: number; line: number }; lines: number; publicApi: boolean }[];
  candidates: string[];
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
  [key: string]: unknown;
}

const BOOLEAN_FLAGS = new Set([
  "json", "help", "force", "pr", "in-place", "allow-dirty", "verify", "receipt",
  "quiet", "dry-run", "claude", "cursor", "opencode", "all", "no-checks",
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
  const result = await requestEngine<{ snapshot: Snapshot }>("analyze", { root, config: engineConfig(config) });
  return result.snapshot;
}

async function scanCommand(root: string, args: ParsedArgs): Promise<number> {
  const config = await loadConfig(root);
  const result = await requestEngine<{ snapshot: Snapshot; candidates: unknown[] }>("scan", {
    root, config: engineConfig(config),
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
  cleanup: () => void;
}

async function analyzePair(root: string, args: ParsedArgs): Promise<AnalysisPair> {
  rejectUnknownFlags(args, ["json", "base", "help", "allow-dirty", "quiet"]);
  const config = await loadConfig(root);
  const baseRef = resolveDefaultBase(root, stringFlag(args, "base"));
  const baseCommit = mergeBase(root, baseRef);
  const head = headCommit(root);
  const dirty = isDirty(root);
  if (dirty && !booleanFlag(args, "allow-dirty")) {
    throw new UsageError("working tree is dirty; commit, stash, or pass --allow-dirty");
  }
  const worktree = createWorktree(root, baseCommit, "base");
  try {
    const base = await analyze(worktree.directory, config);
    const headSnapshot = await analyze(root, config);
    const delta = await requestEngine<DeltaReport>("delta", {
      baseSnapshot: base, headSnapshot, baseRef, baseCommit, headCommit: head, dirty,
    });
    return { base, head: headSnapshot, delta, baseRef, baseCommit, headCommit: head, dirty, cleanup: worktree.cleanup };
  } catch (error) {
    worktree.cleanup();
    throw error;
  }
}

async function deltaCommand(root: string, args: ParsedArgs): Promise<number> {
  const pair = await analyzePair(root, args);
  try {
    if (booleanFlag(args, "json")) writeJson(pair.delta);
    else renderDelta(pair.delta);
    return pair.delta.newlyDead.length > 0 || pair.delta.newlyDeadFiles.length > 0
      ? ExitCode.FINDINGS : ExitCode.OK;
  } finally { pair.cleanup(); }
}

async function planCommand(root: string, args: ParsedArgs): Promise<number> {
  const pair = await analyzePair(root, args);
  try {
    const config = await loadConfig(root);
    const plan = await requestEngine<Plan>("plan", { snapshot: pair.head, delta: pair.delta, config: engineConfig(config) });
    mkdirSync(join(root, ".shrike"), { recursive: true });
    writeFileSync(join(root, ".shrike", "last-plan.json"), `${JSON.stringify(plan, null, 2)}\n`);
    if (booleanFlag(args, "json")) writeJson(plan);
    else renderPlan(plan);
    return plan.totals.symbols > 0 ? ExitCode.FINDINGS : ExitCode.OK;
  } finally { pair.cleanup(); }
}

async function pruneCommand(root: string, args: ParsedArgs): Promise<number> {
  rejectUnknownFlags(args, ["json", "base", "help", "allow-dirty", "pr", "in-place", "verify", "receipt", "dry-run", "no-checks"]);
  if (isDirty(root)) throw new UsageError("prune requires a clean working tree");
  const pair = await analyzePair(root, args);
  const config = await loadConfig(root);
  const plan = await requestEngine<Plan>("plan", { snapshot: pair.head, delta: pair.delta, config: engineConfig(config) });
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
  const branch = `shrike/cleanup-${plan.id.replace(/^plan_/, "")}`;
  const useWorktree = !booleanFlag(args, "in-place");
  const worktree = useWorktree ? createBranchWorktree(root, branch, pair.headCommit) : null;
  const workingRoot = worktree?.directory ?? root;
  if (!useWorktree) ensureBranch(root, branch);
  const checks = config.checks.length > 0 ? config.checks : detectRepositoryChecks(workingRoot);
  const baseline = await captureBaseline({ head: pair.headCommit, specs: checks, cwd: workingRoot });
  let outcome: string = "verified";
  const receipts: string[] = [];
  for (const batch of plan.batches) {
    const lines = deleteBatch(workingRoot, pair.head, batch);
    const patchPath = writePatch(root, workingRoot, batch.id);
    if (lines === 0) continue;
    let compared: ComparedCheck[] = [];
    if (!booleanFlag(args, "no-checks") && checks.length > 0) {
      const after = await runChecks(checks, { cwd: workingRoot });
      compared = after.map((result: CheckResult) => {
        const base = baseline.checks.find((check) => check.name === result.name);
        return { ...result, baselineStatus: base?.status ?? "missing", baselineSignature: base?.signature ?? null, classification: classifyCheck(result, base) };
      });
      const blocking = compared.filter((check) => check.required && ["new-failure", "fail", "timeout", "error", "flaky"].includes(check.classification));
      if (blocking.length > 0) {
        outcome = "rejected";
        restoreAll(workingRoot);
        const receipt = makeReceipt(root, pair, plan, batch, compared, "rejected", patchPath);
        receipts.push(writeReceipt(root, receipt));
        worktree?.cleanup();
        if (booleanFlag(args, "json")) writeJson({ plan, outcome, receipts });
        else writeHuman(`Shrike: batch ${batch.id} rejected (${blocking.map((check) => check.name).join(", ")}); changes reverted.`);
        return ExitCode.VERIFICATION_FAILED;
      }
    }
    commitAll(workingRoot, `shrike: remove ${batch.symbols.length} dead symbols (${batch.id})`);
    const receipt = makeReceipt(root, pair, plan, batch, compared, "verified", patchPath);
    receipts.push(writeReceipt(root, receipt));
    if (!booleanFlag(args, "quiet")) writeHuman(`Verified ${batch.id}: ${batch.symbols.length} symbols, ${batch.files.length} files.`);
  }
  if (booleanFlag(args, "pr")) openPullRequest(root, branch, plan);
  if (booleanFlag(args, "json")) writeJson({ plan, outcome, receipts, branch, worktree: worktree?.directory ?? null });
  else {
    writeHuman(`Shrike: ${formatCount(plan.totals.symbols, "symbol")} removed on ${branch}.`);
    if (worktree) writeHuman(`Worktree: ${worktree.directory}`);
    if (!booleanFlag(args, "pr")) writeHuman(`Open a draft PR with: gh pr create --draft --head ${branch}`);
  }
  return ExitCode.OK;
}

function makeReceipt(root: string, pair: AnalysisPair, plan: Plan, batch: Plan["batches"][number], checks: ComparedCheck[], outcome: string, patchPath: string) {
  return buildReceipt({
    tool: { name: "shrike", version: VERSION },
    base: { ref: pair.baseRef, commit: pair.baseCommit, graphHash: pair.base.graphHash },
    head: { commit: pair.headCommit, graphHash: pair.head.graphHash, dirty: pair.dirty },
    plan: { id: plan.id, hash: plan.deltaHash },
    batch: { id: batch.id, symbols: batch.symbols, files: batch.files, linesRemoved: batch.lines, patches: [patchPath] },
    checks: toReceiptChecks(checks),
    outcome: outcome as never,
    rejected: [],
    policy: { entryPolicy: "local", maxSymbols: 50, maxFiles: 10, allowOversized: false },
    environment: captureEnvironment({}),
  });
}

function deleteBatch(root: string, snapshot: Snapshot, batch: Plan["batches"][number]): number {
  let removed = 0;
  for (const file of batch.files) {
    const path = join(root, file);
    if (!existsSync(path)) continue;
    const content = readFileSync(path, "utf8");
    const lines = content.split("\n");
    const ranges: [number, number][] = [];
    for (const key of batch.symbols) {
      const symbol = snapshot.symbols.find((candidate) => candidate.key === key && candidate.file === file);
      if (!symbol) continue;
      const startLine = symbol.span.line;
      const endLine = lineAt(content, symbol.span.end);
      if (startLine > 0) ranges.push([startLine - 1, Math.max(startLine - 1, endLine - 1)]);
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

function lineAt(content: string, offset: number): number {
  let line = 1;
  for (let index = 0; index < Math.min(offset, content.length); index += 1) if (content[index] === "\n") line += 1;
  return line;
}

function writePatch(root: string, workingRoot: string, batchId: string): string {
  mkdirSync(join(root, ".shrike", "patches"), { recursive: true });
  const relative = `.shrike/patches/${batchId}.patch`;
  writeFileSync(join(root, relative), execGitDiff(workingRoot));
  return relative;
}

function execGitDiff(root: string): string {
  try {
    return execFileSync("git", ["diff"], { cwd: root, encoding: "utf8" });
  } catch { return ""; }
}

function openPullRequest(root: string, branch: string, plan: Plan): void {
  const base = plan.base.ref ?? "main";
  const title = `Shrike: remove ${plan.totals.symbols} dead symbols`;
  const body = `Verified deletion plan ${plan.id}. ${plan.totals.files} files, ${plan.totals.lines} lines.\n\nEvery batch was verified against its baseline; see .shrike/receipts/.`;
  const push = spawnSync("git", ["push", "-u", "origin", branch], { cwd: root, encoding: "utf8" });
  if (push.status !== 0) {
    writeStderr(`Could not push ${branch}; open the PR manually: gh pr create --draft --head ${branch} --base ${base}`);
    return;
  }
  const gh = spawnSync("gh", ["pr", "create", "--draft", "--head", branch, "--base", base, "--title", title, "--body", body], { cwd: root, encoding: "utf8" });
  if (gh.status === 0) writeHuman(gh.stdout.trim());
  else writeStderr(`gh unavailable or failed; open the PR manually: gh pr create --draft --head ${branch} --base ${base}`);
}

async function verifyCommand(root: string, args: ParsedArgs): Promise<number> {
  const id = stringFlag(args, "plan") ?? args.positionals[0];
  if (!id) throw new UsageError("verify requires --plan <id> or a receipt id");
  const receipt = readReceipt(join(root, ".shrike", "receipts", `${id}.json`));
  if (booleanFlag(args, "json")) writeJson(receipt);
  else writeHuman(`Receipt ${receipt.id}: ${receipt.outcome}`);
  return receipt.outcome === "verified" ? ExitCode.OK : ExitCode.VERIFICATION_FAILED;
}

function receiptCommand(root: string, args: ParsedArgs): number {
  const id = args.positionals[0];
  if (!id) {
    const receipts = listReceipts(root);
    if (booleanFlag(args, "json")) writeJson({ receipts });
    else writeHuman(receipts.length > 0 ? receipts.join("\n") : "No receipts.");
    return ExitCode.OK;
  }
  const receipt = readReceipt(join(root, ".shrike", "receipts", `${id}.json`));
  if (booleanFlag(args, "json")) writeJson(receipt);
  else writeHuman(`${receipt.id} ${receipt.outcome} ${receipt.createdAt}`);
  return ExitCode.OK;
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
  const buildPath = stringFlag(args, "build");
  if (!buildPath) throw new UsageError("evidence requires --build <path>");
  const config = await loadConfig(root);
  const snapshot = await analyze(root, config);
  const build = JSON.parse(readFileSync(join(root, buildPath), "utf8"));
  const result = await requestEngine("join-build", { snapshot, build });
  if (booleanFlag(args, "json")) writeJson(result);
  else writeHuman(JSON.stringify(result, null, 2));
  return ExitCode.OK;
}

function hookCommand(root: string, args: ParsedArgs): number {
  if (args.positionals[0] !== "install") throw new UsageError("usage: shrike hook install --claude|--cursor|--opencode");
  const target: HookTarget | null = booleanFlag(args, "claude") ? "claude" : booleanFlag(args, "cursor") ? "cursor" : booleanFlag(args, "opencode") ? "opencode" : null;
  if (!target) throw new UsageError("choose --claude, --cursor, or --opencode");
  const result = installHook(root, target);
  if (booleanFlag(args, "json")) writeJson(result);
  else writeHuman(`Installed ${target} hook in ${result.path}`);
  return ExitCode.OK;
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
    if (request.method === "initialize") {
      respond({ protocolVersion: "2024-11-05", capabilities: { tools: {} }, serverInfo: { name: "shrike", version: VERSION } });
    } else if (request.method === "tools/list") {
      respond({ tools: [
        { name: "shrike_scan", description: "Current-state dead-code evidence", inputSchema: { type: "object", properties: { json: { type: "boolean" } } } },
        { name: "shrike_delta", description: "Whole-graph PR orphan delta", inputSchema: { type: "object", properties: { base: { type: "string" } } } },
        { name: "shrike_plan", description: "Verified deletion batches", inputSchema: { type: "object", properties: { base: { type: "string" } } } },
        { name: "shrike_proof", description: "Witness or proof trace", inputSchema: { type: "object", properties: { symbol: { type: "string" } }, required: ["symbol"] } },
      ] });
    } else if (request.method === "tools/call") {
      const name = request.params?.name ?? "";
      const command = name.replace(/^shrike_/, "");
      if (!["scan", "delta", "plan", "proof"].includes(command)) {
        process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: request.id ?? null, error: { code: -32601, message: "unknown tool" } })}\n`);
        continue;
      }
      const toolArgs = (request.params?.arguments ?? {}) as Record<string, unknown>;
      const argv = [process.execPath, cli, command, "--json"];
      if (typeof toolArgs.base === "string") argv.push("--base", toolArgs.base);
      if (typeof toolArgs.symbol === "string") argv.push(toolArgs.symbol);
      const output = await runCli(root, argv);
      respond({ content: [{ type: "text", text: output.stdout }], isError: output.code !== 0 });
    } else if (request.id !== undefined) {
      process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: request.id, error: { code: -32601, message: "method not found" } })}\n`);
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

function cacheCommand(root: string, args: ParsedArgs): number {
  const action = args.positionals[0];
  const directory = join(root, ".shrike");
  if (action === "clean") {
    rmSync(directory, { recursive: true, force: true });
    if (booleanFlag(args, "json")) writeJson({ cleaned: directory });
    else writeHuman("Cache cleaned.");
    return ExitCode.OK;
  }
  if (action === "verify") {
    const exists = existsSync(directory);
    if (booleanFlag(args, "json")) writeJson({ exists, path: directory });
    else writeHuman(exists ? `Cache present at ${directory}` : "Cache is empty.");
    return exists ? ExitCode.OK : ExitCode.FINDINGS;
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
  prune --base <ref>   Apply, verify, commit batches (branch only)
  verify --plan <id>   Re-check a plan/receipt
  receipt [id]         Show receipts
  proof <symbol>       Witness path or proof trace
  evidence --build <path>  Join build-graph evidence
  hook install --claude|--cursor|--opencode
  mcp serve            MCP server over stdio
  cache clean|verify   Cache maintenance
  version              Version and protocol

All read-only commands support --json. Exit codes: 0 clean, 1 findings,
2 usage, 3 verification failed, 4 could not run, 5 revert failed.`);
}

main(process.argv.slice(2))
  .then((code) => process.exit(code))
  .catch((error: unknown) => {
    const cliError = isCliError(error) ? error : new CliError(ExitCode.COULD_NOT_RUN, "internal", error instanceof Error ? error.message : String(error));
    writeStderr(`shrike: ${cliError.message}`);
    if (process.argv.includes("--json")) writeJsonError({ code: cliError.code, message: cliError.message, exitCode: cliError.exitCode, details: cliError.details });
    process.exit(cliError.exitCode);
  });
