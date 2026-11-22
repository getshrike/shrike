import { spawnSync } from "node:child_process";

import { resolveCli } from "./cli.ts";
import { isForkPullRequest, writeOutput, writeSummary } from "./inputs.ts";

const BOT_IDENTITY = {
  GIT_AUTHOR_NAME: "github-actions[bot]",
  GIT_AUTHOR_EMAIL: "41898282+github-actions[bot]@users.noreply.github.com",
  GIT_COMMITTER_NAME: "github-actions[bot]",
  GIT_COMMITTER_EMAIL: "41898282+github-actions[bot]@users.noreply.github.com",
};

/**
 * Git refuses to commit without an identity, and a fresh runner has none.
 * Keep a configured identity, but fall back to the GitHub Actions bot.
 */
export function commitEnv(root: string): NodeJS.ProcessEnv {
  const author = spawnSync("git", ["var", "GIT_AUTHOR_IDENT"], { cwd: root, encoding: "utf8" });
  const committer = spawnSync("git", ["var", "GIT_COMMITTER_IDENT"], { cwd: root, encoding: "utf8" });
  if (author.status === 0 && committer.status === 0) return { ...process.env };
  return { ...process.env, ...BOT_IDENTITY };
}

export function runPrune(root: string, base: string): number {
  if (isForkPullRequest()) {
    process.stderr.write("shrike: refusing prune on a fork pull request (write token is not trusted)\n");
    return 2;
  }
  const cli = resolveCli(root);
  const result = spawnSync(cli.command, [...cli.args, "prune", "--base", base, "--json", "--pr"], {
    cwd: root,
    encoding: "utf8",
    env: commitEnv(root),
  });
  const stdout = result.stdout ?? "";
  const stderr = result.stderr ?? "";
  if (result.error) process.stderr.write(`shrike: failed to run ${cli.command}: ${result.error.message}\n`);
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
