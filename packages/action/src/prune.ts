import { spawnSync } from "node:child_process";

import { resolveCli } from "./cli.ts";
import { isForkPullRequest, writeOutput, writeSummary } from "./inputs.ts";

export function runPrune(root: string, base: string): number {
  if (isForkPullRequest()) {
    process.stderr.write("shrike: refusing prune on a fork pull request (write token is not trusted)\n");
    return 2;
  }
  const cli = resolveCli(root);
  const result = spawnSync(cli.command, [...cli.args, "prune", "--base", base, "--json", "--pr"], {
    cwd: root,
    encoding: "utf8",
  });
  const stdout = result.stdout ?? "";
  const stderr = result.stderr ?? "";
  if (stderr.trim()) process.stderr.write(stderr);
  let outcome = "unknown";
  let receipts: string[] = [];
  try {
    const parsed = JSON.parse(stdout) as { outcome?: string; receipts?: string[] };
    outcome = parsed.outcome ?? "unknown";
    receipts = parsed.receipts ?? [];
  } catch {
    // The CLI exit code is still authoritative.
  }
  writeSummary(
    `## Shrike verified cleanup\n\nOutcome: **${outcome}**\n\n` +
      (receipts.length > 0 ? `Receipts: ${receipts.length}\n` : ""),
  );
  writeOutput("outcome", outcome);
  writeOutput("receipts", String(receipts.length));
  return result.status ?? 4;
}
