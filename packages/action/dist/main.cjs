#!/usr/bin/env node
"use strict";

// src/inputs.ts
var import_node_fs = require("node:fs");
function input(name, fallback = "") {
  return process.env[`INPUT_${name.toUpperCase().replace(/-/g, "_")}`] ?? fallback;
}
function writeOutput(name, value) {
  if (process.env.GITHUB_OUTPUT) (0, import_node_fs.appendFileSync)(process.env.GITHUB_OUTPUT, `${name}=${value}
`);
}
function writeSummary(text) {
  const path = process.env.GITHUB_STEP_SUMMARY;
  if (path) (0, import_node_fs.appendFileSync)(path, `${text}
`);
}
function isForkPullRequest() {
  const eventName = process.env.GITHUB_EVENT_NAME ?? "";
  if (!eventName.startsWith("pull_request")) return false;
  const path = process.env.GITHUB_EVENT_PATH;
  if (!path || !(0, import_node_fs.existsSync)(path)) return true;
  try {
    const event = JSON.parse((0, import_node_fs.readFileSync)(path, "utf8"));
    const head = event.pull_request?.head?.repo?.full_name;
    const base = event.repository?.full_name;
    return typeof head === "string" && typeof base === "string" && head !== base;
  } catch {
    return true;
  }
}

// src/delta.ts
var import_node_child_process2 = require("node:child_process");
var import_node_fs3 = require("node:fs");
var import_node_path2 = require("node:path");

// src/cli.ts
var import_node_child_process = require("node:child_process");
var import_node_fs2 = require("node:fs");
var import_node_module = require("node:module");
var import_node_path = require("node:path");
function resolveCli(root) {
  const configured = process.env.SHRIKE_CLI;
  if (configured) {
    return /\.(?:c|m)?js$/i.test(configured) ? { command: process.execPath, args: [configured] } : { command: configured, args: [] };
  }
  try {
    const require2 = (0, import_node_module.createRequire)((0, import_node_path.join)(root, "package.json"));
    const manifest = require2.resolve("getshrike/package.json");
    const launcher = (0, import_node_path.join)((0, import_node_path.dirname)(manifest), "bin", "shrike.mjs");
    if ((0, import_node_fs2.existsSync)(launcher)) return { command: process.execPath, args: [launcher] };
  } catch {
  }
  const probe = (0, import_node_child_process.spawnSync)(process.platform === "win32" ? "where" : "which", ["shrike"], {
    cwd: root,
    encoding: "utf8"
  });
  if (probe.status === 0) return { command: "shrike", args: [] };
  throw new Error(
    "shrike was not found. Run `npm install -D getshrike` before this action, or set SHRIKE_CLI to the launcher path."
  );
}

// src/delta.ts
function runDelta(root, base, maxItems, failOn) {
  const jsonPath = (0, import_node_path2.join)(root, ".shrike", "action-delta.json");
  (0, import_node_fs3.mkdirSync)((0, import_node_path2.dirname)(jsonPath), { recursive: true });
  const cli = resolveCli(root);
  const result = (0, import_node_child_process2.spawnSync)(
    cli.command,
    [...cli.args, "delta", "--base", base, "--json", "--allow-dirty", "--fail-on", "never"],
    { cwd: root, encoding: "utf8" }
  );
  const stdout = result.stdout ?? "";
  const stderr = result.stderr ?? "";
  if (result.error) process.stderr.write(`shrike: failed to run ${cli.command}: ${result.error.message}
`);
  if (stderr.trim()) process.stderr.write(stderr);
  let summary = "## Shrike orphan delta\n\n";
  let count = 0;
  let delta;
  try {
    delta = JSON.parse(stdout);
  } catch {
    summary += `Delta did not produce JSON (exit ${String(result.status)}).
`;
    (0, import_node_fs3.rmSync)(jsonPath, { force: true });
    writeSummary(summary);
    writeOutput("count", "0");
    writeOutput("delta-path", "");
    return result.status !== null && result.status > 1 ? result.status : 4;
  }
  count = (delta.newlyDead?.length ?? 0) + (delta.newlyDeadFiles?.length ?? 0);
  if (delta.incomparable) summary += "**Incomparable snapshots** \u2014 no deletion claim made.\n";
  else if (count === 0) summary += "No newly dead code.\n";
  else {
    summary += `**${count}** newly dead item(s).

| Symbol | File | Lines |
|---|---|---|
`;
    for (const item of delta.newlyDead ?? []) {
      summary += `| \`${item.symbol}\` | \`${item.file}\` | ${item.estimatedLines} |
`;
    }
    for (const file of delta.newlyDeadFiles ?? []) summary += `| (file) | \`${file}\` | \u2014 |
`;
  }
  if (stdout.trim()) (0, import_node_fs3.writeFileSync)(jsonPath, stdout);
  else (0, import_node_fs3.rmSync)(jsonPath, { force: true });
  writeSummary(summary);
  writeOutput("count", String(count));
  if ((0, import_node_fs3.existsSync)(jsonPath)) writeOutput("delta-path", jsonPath);
  if (result.status !== null && result.status > 1) return result.status;
  if (maxItems > 0 && count > maxItems) return 1;
  if (failOn === "newly-dead" && count > 0) return 1;
  return 0;
}

// src/prune.ts
var import_node_child_process3 = require("node:child_process");
function runPrune(root, base) {
  if (isForkPullRequest()) {
    process.stderr.write("shrike: refusing prune on a fork pull request (write token is not trusted)\n");
    return 2;
  }
  const cli = resolveCli(root);
  const result = (0, import_node_child_process3.spawnSync)(cli.command, [...cli.args, "prune", "--base", base, "--json", "--pr"], {
    cwd: root,
    encoding: "utf8"
  });
  const stdout = result.stdout ?? "";
  const stderr = result.stderr ?? "";
  if (result.error) process.stderr.write(`shrike: failed to run ${cli.command}: ${result.error.message}
`);
  if (stderr.trim()) process.stderr.write(stderr);
  let outcome = "unknown";
  let receipts = [];
  try {
    const parsed = JSON.parse(stdout);
    outcome = parsed.outcome ?? "unknown";
    receipts = parsed.receipts ?? [];
  } catch {
  }
  writeSummary(
    `## Shrike verified cleanup

Outcome: **${outcome}**

` + (receipts.length > 0 ? `Receipts: ${receipts.length}
` : "")
  );
  writeOutput("outcome", outcome);
  writeOutput("receipts", String(receipts.length));
  return result.status ?? 4;
}

// src/main.ts
function run() {
  const root = process.env.GITHUB_WORKSPACE ?? process.cwd();
  const base = input("base") || process.env.GITHUB_BASE_REF || "origin/main";
  const mode = input("mode", "delta");
  const maxItems = Number(input("max-items", "0"));
  const failOn = input("fail-on", "never");
  if (mode !== "delta" && mode !== "prune") {
    process.stderr.write(`shrike: unknown mode ${mode}; expected delta or prune
`);
    return 2;
  }
  if (mode === "delta" && failOn !== "newly-dead" && failOn !== "never") {
    process.stderr.write(`shrike: unknown fail-on ${failOn}; expected newly-dead or never
`);
    return 2;
  }
  try {
    return mode === "prune" ? runPrune(root, base) : runDelta(root, base, maxItems, failOn);
  } catch (error) {
    process.stderr.write(`shrike: ${error instanceof Error ? error.message : String(error)}
`);
    return 4;
  }
}
process.exit(run());
