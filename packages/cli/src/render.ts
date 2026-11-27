import { formatCount, writeHuman } from "./output.ts";
import { VERSION } from "./version.ts";
import type { DeltaReport, Plan } from "./cli-types.ts";

export function renderDelta(delta: DeltaReport): void {
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

export function renderPlan(plan: Plan): void {
  writeHuman(`Plan ${plan.id}: ${plan.totals.symbols} symbols, ${plan.totals.files} files, ${plan.totals.lines} lines`);
  for (const batch of plan.batches) {
    writeHuman(`  ${batch.id}: ${batch.symbols.length} symbols, ${batch.files.length} files, ${batch.lines} lines`);
  }
  if (plan.blocked.length > 0) writeHuman(`Blocked: ${formatCount(plan.blocked.length, "item")}`);
}

export function printHelp(): void {
  writeHuman(`shrike ${VERSION} — the cleanup layer for AI-built TypeScript apps

Usage: shrike <command> [options]

Commands:
  init                 Create shrike.config.ts and ignore .shrike/
  scan                 Current-state dead-code evidence
  deps                 Unused and unlisted dependencies
  delta --base <ref>   Whole-graph PR orphan delta
  plan --base <ref>    Deletion batches and blast radius
  report [--base <ref>]  Dead-weight report (plain; run with no command for TUI)
  clean [--base <ref>] [--build <path>]  Apply, verify, commit batches (branch only)
  clean --all          Current-state cleanup through the same verified pipeline
  why <path#Name>      Human proof for one symbol; --json is the engine proof
  prune --base <ref> [--build <path>]  Apply, verify, commit batches (branch only)
  install [--target <name>]  Write the agent skill and optional hook
  ci install [--gate]  Write the advisory GitHub Actions workflow
  verify --plan <id>   Re-check a plan/receipt
  receipt [id]         Show receipts
  proof <path#Name>    Witness path or proof trace
  evidence [--build <path>] [--require-build-evidence]  Join build-graph evidence
  hook install --claude|--cursor|--opencode
  mcp serve            MCP server over stdio
  cache clean|verify   Cache maintenance
  version              Version and protocol

All read-only commands support --json. Exit codes: 0 clean, 1 findings,
2 usage, 3 verification failed, 4 could not run, 5 revert failed.`);
}

export function printCleanupHelp(): void {
  writeHuman(`shrike clean — verified dead-code cleanup

Usage:
  shrike clean [--base <ref>] [--build <path>] [--dry-run] [--json]
               [--in-place] [--allow-dirty] [--pr] [--tier <tier>]
  shrike clean --all [--dry-run] [--json] [--in-place] [--allow-dirty]
                    [--build <path>] [--pr] [--tier <tier>]

Without --all, clean resolves a branch base and removes code that this branch
orphaned. With --all, clean plans every removable file and symbol in the
current tree. Both paths run project checks before and after, commit each
verified batch, and write a receipt. --all cannot be combined with --base.

Exit codes: 0 clean, 1 findings (--dry-run), 2 usage,
3 verification failed, 4 could not run, 5 revert failed.`);
}

export function printInstallHelp(): void {
  writeHuman(`shrike install — agent skill and hook setup

Usage:
  shrike install [--dry-run] [--force] [--json]
                 [--target <target>]...
                 [--no-skill] [--no-hook] [--no-scripts] [--scripts]
                 [--claude] [--cursor] [--opencode]

Targets: agents, claude, cursor, opencode, codex, gemini, copilot.
With no target, Shrike detects repository markers and otherwise installs the
portable skill at .agents/skills/shrike/SKILL.md. The installer never runs
analysis, project scripts, or network requests.`);
}

export function printCiHelp(): void {
  writeHuman(`shrike ci install — GitHub Actions setup

Usage:
  shrike ci install [--gate] [--dry-run] [--force] [--json]
                    [--path <workflow-path>] [--base <ref>] [--max-items <n>]

Writes .github/workflows/shrike.yml. The default is advisory (fail-on: never)
with contents: read. --gate makes newly dead code blocking and adds
pull-requests: write for the PR comment. It never prunes or uses secrets.`);
}
