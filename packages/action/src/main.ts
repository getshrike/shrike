#!/usr/bin/env node
/**
 * GitHub Action entry point.
 *
 * Delta mode is read-only and uploads the JSON artifact. Prune mode drives the
 * full verified deletion flow on the checked-out branch. Both modes are thin
 * wrappers over the CLI; the Action never re-implements analysis.
 */
import { input } from "./inputs.ts";
import { runDelta } from "./delta.ts";
import { runPrune } from "./prune.ts";

function run(): number {
  const root = process.env.GITHUB_WORKSPACE ?? process.cwd();
  const base = input("base") || process.env.GITHUB_BASE_REF || "origin/main";
  const mode = input("mode", "delta");
  const maxItems = Number(input("max-items", "0"));
  const failOn = input("fail-on", "never");
  if (mode === "prune") return runPrune(root, base);
  if (mode !== "delta") {
    process.stderr.write(`shrike: unknown mode ${mode}; expected delta or prune\n`);
    return 2;
  }
  if (failOn !== "newly-dead" && failOn !== "never") {
    process.stderr.write(`shrike: unknown fail-on ${failOn}; expected newly-dead or never\n`);
    return 2;
  }
  return runDelta(root, base, maxItems, failOn);
}

process.exit(run());
