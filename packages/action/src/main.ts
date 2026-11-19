#!/usr/bin/env node
/**
 * GitHub Action entry point.
 *
 * Delta mode is read-only and uploads the JSON artifact. Prune mode drives the
 * full verified deletion flow on the checked-out branch. Both modes are thin
 * wrappers over the CLI; the Action never re-implements analysis.
 */
import { spawnSync } from "node:child_process";
import { appendFileSync, existsSync, mkdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";

function input(name: string, fallback = ""): string {
  return process.env[`INPUT_${name.toUpperCase().replace(/-/g, "_")}`] ?? fallback;
}

function writeOutput(name: string, value: string): void {
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `${name}=${value}\n`);
}

function writeSummary(text: string): void {
  const path = process.env.GITHUB_STEP_SUMMARY;
  if (path) appendFileSync(path, `${text}\n`);
}

/**
 * Refuse write-capable flows on a fork PR. The event file is authoritative; a
 * missing or unreadable event on a PR fails closed.
 */
function isForkPullRequest(): boolean {
  const eventName = process.env.GITHUB_EVENT_NAME ?? "";
  if (!eventName.startsWith("pull_request")) return false;
  const path = process.env.GITHUB_EVENT_PATH;
  if (!path || !existsSync(path)) return true;
  try {
    const event = JSON.parse(readFileSync(path, "utf8")) as {
      pull_request?: { head?: { repo?: { full_name?: string } } };
      repository?: { full_name?: string };
    };
    const head = event.pull_request?.head?.repo?.full_name;
    const base = event.repository?.full_name;
    return typeof head === "string" && typeof base === "string" && head !== base;
  } catch {
    return true;
  }
}

function runDelta(root: string, base: string, maxItems: number): number {
  const jsonPath = join(root, ".shrike", "action-delta.json");
  mkdirSync(dirname(jsonPath), { recursive: true });
  const result = spawnSync("shrike", ["delta", "--base", base, "--json", "--allow-dirty"], {
    cwd: root,
    encoding: "utf8",
  });
  const stdout = result.stdout ?? "";
  const stderr = result.stderr ?? "";
  if (stdout.trim()) appendFileSync(jsonPath, stdout);
  if (stderr.trim()) process.stderr.write(stderr);

  let summary = "## Shrike orphan delta\n\n";
  let count = 0;
  try {
    const delta = JSON.parse(stdout) as {
      newlyDead?: { symbol: string; file: string; estimatedLines: number }[];
      newlyDeadFiles?: string[];
      incomparable?: unknown;
    };
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
  } catch {
    summary += `Delta did not produce JSON (exit ${String(result.status)}).\n`;
  }
  writeSummary(summary);
  writeOutput("count", String(count));
  if (existsSync(jsonPath)) writeOutput("delta-path", jsonPath);
  if (result.status === 4) return 4;
  if (maxItems > 0 && count > maxItems) return 1;
  return 0;
}

function runPrune(root: string, base: string): number {
  if (isForkPullRequest()) {
    process.stderr.write("shrike: refusing prune on a fork pull request (write token is not trusted)\n");
    return 2;
  }
  const result = spawnSync("shrike", ["prune", "--base", base, "--json", "--pr"], {
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

function run(): number {
  const root = process.env.GITHUB_WORKSPACE ?? process.cwd();
  const base = input("base") || process.env.GITHUB_BASE_REF || "origin/main";
  const mode = input("mode", "delta");
  const maxItems = Number(input("max-items", "0"));
  if (mode === "prune") return runPrune(root, base);
  if (mode !== "delta") {
    process.stderr.write(`shrike: unknown mode ${mode}; expected delta or prune\n`);
    return 2;
  }
  return runDelta(root, base, maxItems);
}

process.exit(run());
