#!/usr/bin/env node
/**
 * GitHub Action entry point.
 *
 * The action is a thin wrapper over `shrike delta`; it never re-implements
 * analysis. It writes a job summary and fails when the delta exceeds the
 * configured threshold.
 */
import { spawnSync } from "node:child_process";
import { appendFileSync, existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

function input(name: string, fallback = ""): string {
  return process.env[`INPUT_${name.toUpperCase().replace(/-/g, "_")}`] ?? fallback;
}

function run(): number {
  const root = process.env.GITHUB_WORKSPACE ?? process.cwd();
  const base = input("base") || process.env.GITHUB_BASE_REF || "origin/main";
  const maxItems = Number(input("max-items", "0"));
  const jsonPath = join(root, ".shrike", "action-delta.json");
  const result = spawnSync("shrike", ["delta", "--base", base, "--json", "--allow-dirty"], {
    cwd: root,
    encoding: "utf8",
  });
  const stdout = result.stdout ?? "";
  const stderr = result.stderr ?? "";
  if (stdout.trim()) {
    appendFileSync(jsonPath, stdout);
  }
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
  const summaryPath = process.env.GITHUB_STEP_SUMMARY;
  if (summaryPath) appendFileSync(summaryPath, `${summary}\n`);
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `count=${count}\n`);
  if (existsSync(jsonPath) && process.env.GITHUB_OUTPUT) {
    appendFileSync(process.env.GITHUB_OUTPUT, `delta-path=${jsonPath}\n`);
  }
  if (result.status === 4) return 4;
  if (maxItems > 0 && count > maxItems) return 1;
  return 0;
}

process.exit(run());
