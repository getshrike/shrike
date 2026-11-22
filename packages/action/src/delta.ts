import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";

import { resolveCli } from "./cli.ts";
import { writeOutput, writeSummary } from "./inputs.ts";

export function runDelta(root: string, base: string, maxItems: number, failOn: string): number {
  const jsonPath = join(root, ".shrike", "action-delta.json");
  mkdirSync(dirname(jsonPath), { recursive: true });
  const cli = resolveCli(root);
  const result = spawnSync(
    cli.command,
    [...cli.args, "delta", "--base", base, "--json", "--allow-dirty", "--fail-on", "never"],
    { cwd: root, encoding: "utf8" },
  );
  const stdout = result.stdout ?? "";
  const stderr = result.stderr ?? "";
  if (stderr.trim()) process.stderr.write(stderr);

  let summary = "## Shrike orphan delta\n\n";
  let count = 0;
  let delta: {
    newlyDead?: { symbol: string; file: string; estimatedLines: number }[];
    newlyDeadFiles?: string[];
    incomparable?: unknown;
  };
  try {
    delta = JSON.parse(stdout) as typeof delta;
  } catch {
    summary += `Delta did not produce JSON (exit ${String(result.status)}).\n`;
    rmSync(jsonPath, { force: true });
    writeSummary(summary);
    writeOutput("count", "0");
    writeOutput("delta-path", "");
    return result.status !== null && result.status > 1 ? result.status : 4;
  }

  count = (delta.newlyDead?.length ?? 0) + (delta.newlyDeadFiles?.length ?? 0);
  if (delta.incomparable) summary += "**Incomparable snapshots** — no deletion claim made.\n";
  else if (count === 0) summary += "No newly dead code.\n";
  else {
    summary += `**${count}** newly dead item(s).\n\n| Symbol | File | Lines |\n|---|---|---|\n`;
    for (const item of delta.newlyDead ?? []) {
      summary += `| \`${item.symbol}\` | \`${item.file}\` | ${item.estimatedLines} |\n`;
    }
    for (const file of delta.newlyDeadFiles ?? []) summary += `| (file) | \`${file}\` | — |\n`;
  }

  // Replace the artifact on every run; appending would corrupt the JSON on a
  // rerun. A failed invocation removes any stale artifact rather than exposing
  // an old delta as current evidence.
  if (stdout.trim()) writeFileSync(jsonPath, stdout);
  else rmSync(jsonPath, { force: true });
  writeSummary(summary);
  writeOutput("count", String(count));
  if (existsSync(jsonPath)) writeOutput("delta-path", jsonPath);
  if (result.status !== null && result.status > 1) return result.status;
  if (maxItems > 0 && count > maxItems) return 1;
  if (failOn === "newly-dead" && count > 0) return 1;
  return 0;
}
