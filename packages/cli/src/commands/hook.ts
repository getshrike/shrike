import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { booleanFlag, type ParsedArgs } from "../args.ts";
import { loadConfig, type ShrikeConfig } from "../config.ts";
import { ExitCode, UsageError } from "../errors.ts";
import { formatCount, writeHuman, writeJson } from "../output.ts";
import { resolveInside } from "../recovery.ts";
import { git, headCommit, isDirty, mergeBase, resolveDefaultBase } from "../git.ts";
import {
  classifyCheck, detectRepositoryChecks, runCheckWithReruns,
  type Baseline, type CheckResult, type CheckSpec, type ComparedCheck,
} from "@shrike/verifier";
import { buildCleanupBrief, installHook, uninstallHook, type HookTarget } from "@shrike/agent";

export const HOOK_TOTAL_BUDGET_MS = 60_000;
export const HOOK_CHECKS_BUDGET_MS = 40_000;

export function environmentMs(name: string, fallback: number): number {
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
export function hookBaseline(
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

export function hookCheckSummary(results: ComparedCheck[], baseline: Baseline | null): string {
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
export async function runHookChecks(
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

export async function hookCommand(root: string, args: ParsedArgs): Promise<number> {
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

export async function hookRunCommand(root: string, args: ParsedArgs): Promise<number> {
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
  const cli = join(import.meta.dirname, "..", "main.ts");
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
