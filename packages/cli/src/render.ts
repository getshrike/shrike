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
