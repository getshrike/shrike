#!/usr/bin/env node
/**
 * End-to-end `clean --all` gate.
 *
 * Builds throwaway git repositories and drives the real CLI and engine through
 * current-state cleanup: dry-run projection, verified deletion, receipt,
 * protected/live code, untouched dependencies, and the empty re-run.
 *
 * Requires target/debug/shrike-engine or SHRIKE_ENGINE.
 */
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const cli = join(root, "packages", "cli", "src", "main.ts");
const engine = process.env.SHRIKE_ENGINE ?? join(root, "target", "debug", "shrike-engine");
if (!existsSync(engine)) {
  console.error(`SKIP e2e-clean-all: engine not found at ${engine}`);
  process.exit(0);
}

function git(cwd, args) {
  const result = spawnSync("git", args, { cwd, encoding: "utf8" });
  if (result.status !== 0) {
    throw new Error(`git ${args.join(" ")} failed: ${result.stderr.trim()}`);
  }
  return result.stdout.trim();
}

function write(cwd, path, content) {
  const full = join(cwd, path);
  mkdirSync(dirname(full), { recursive: true });
  writeFileSync(full, content);
}

function read(cwd, path) {
  return readFileSync(join(cwd, path), "utf8");
}

function run(cwd, args) {
  return spawnSync(process.execPath, [cli, ...args], {
    cwd,
    encoding: "utf8",
    env: { ...process.env, SHRIKE_ENGINE: engine, NO_COLOR: "1" },
  });
}

function makeRepo(name) {
  const directory = mkdtempSync(join(tmpdir(), `shrike-clean-all-${name}-`));
  git(directory, ["init", "-q", "-b", "main"]);
  git(directory, ["config", "user.email", "e2e@shrike.local"]);
  git(directory, ["config", "user.name", "E2E"]);
  return directory;
}

function fixture(directory, { checks = true } = {}) {
  write(directory, "src/index.ts", [
    'import { live } from "./live.ts";',
    'import { kept } from "./mixed.ts";',
    "export const value = live + kept;",
    "",
  ].join("\n"));
  write(directory, "src/live.ts", "export const live = 1;\n");
  write(directory, "src/mixed.ts", "export const kept = 5;\nexport function deadExport() { return 2; }\n");
  write(directory, "src/orphan.ts", "export function orphan() { return 3; }\n");
  write(directory, "src/public.ts", "export function publicApi() { return 4; }\n");
  write(directory, "package.json", `${JSON.stringify({
    name: "clean-all-e2e",
    private: true,
    type: "module",
    main: "./src/index.ts",
    exports: { ".": "./src/public.ts", "./index": "./src/index.ts" },
    dependencies: { "left-pad": "^1.3.0" },
  }, null, 2)}\n`);
  if (checks) {
    write(directory, "shrike.config.json", `${JSON.stringify({
      checks: [{
        name: "smoke",
        command: "node -e \"process.exit(require('fs').existsSync('src/live.ts')?0:1)\"",
        required: true,
        timeoutMs: 30_000,
      }],
    }, null, 2)}\n`);
  }
  git(directory, ["add", "-A"]);
  git(directory, ["commit", "-q", "-m", "fixture"]);
}

function scenario(name, fn) {
  try {
    fn();
    console.log(`ok ${name}`);
  } catch (error) {
    console.error(`not ok ${name}`);
    throw error;
  }
}

scenario("current cleanup dry-run, apply, receipt, and empty re-run", () => {
  const directory = makeRepo("in-place");
  try {
    fixture(directory);
    const packageBefore = read(directory, "package.json");

    const dry = run(directory, ["clean", "--all", "--dry-run", "--json"]);
    assert.equal(dry.status, 1, `dry-run exit: ${dry.stderr}`);
    const planned = JSON.parse(dry.stdout);
    const symbols = planned.batches.flatMap((batch) => batch.symbols);
    const files = planned.batches.flatMap((batch) => batch.files);
    assert(symbols.includes("src/mixed.ts#deadExport"), "dead symbol missing from plan");
    assert(symbols.includes("src/orphan.ts#__module__"), "orphan module key missing from plan");
    assert(files.includes("src/orphan.ts"), "orphan file missing from plan");
    assert(!files.includes("src/public.ts"), "public API file entered the plan");
    assert(!symbols.some((symbol) => symbol.includes("publicApi")), "public API symbol entered the plan");
    assert(!JSON.stringify(planned).includes("left-pad"), "dependency entered the deletion plan");

    const result = run(directory, ["clean", "--all", "--in-place", "--json"]);
    assert.equal(result.status, 0, `apply exit: ${result.stderr}`);
    const output = JSON.parse(result.stdout);
    assert.equal(output.scope, "current");
    assert.equal(output.outcome, "verified");
    assert.equal(output.worktree, null);
    assert.equal(output.receipts.length, 1);
    assert.match(output.receipts[0], /^\.shrike\/receipts\/rcpt_[A-Za-z0-9]+\.json$/);
    assert.ok(existsSync(join(directory, output.receipts[0])), "receipt file is missing");
    assert.equal(git(directory, ["branch", "--show-current"]), output.branch);

    assert.equal(existsSync(join(directory, "src/orphan.ts")), false, "orphan file was not deleted");
    const mixed = read(directory, "src/mixed.ts");
    assert.equal(mixed.includes("deadExport"), false, "dead symbol was not deleted");
    assert.equal(mixed.includes("kept"), true, "live sibling symbol was deleted");
    assert.equal(existsSync(join(directory, "src/public.ts")), true, "public API file was deleted");
    assert.equal(read(directory, "src/public.ts").includes("publicApi"), true, "public API symbol was deleted");
    assert.equal(read(directory, "package.json"), packageBefore, "package.json was modified");

    const receipt = JSON.parse(read(directory, output.receipts[0]));
    assert.equal(receipt.outcome, "verified");
    assert.equal(receipt.base.ref, "HEAD");
    assert.ok(receipt.batch.symbols.includes("src/mixed.ts#deadExport"), "receipt missing dead symbol");
    assert.ok(receipt.batch.symbols.includes("src/orphan.ts#__module__"), "receipt missing whole-file symbol");
    assert.equal(receipt.checks[0].classification, "pass", "receipt check did not pass");
    assert.deepEqual(receipt.rejected, []);

    const again = run(directory, ["clean", "--all", "--json"]);
    assert.equal(again.status, 0, `re-run exit: ${again.stderr}`);
    const empty = JSON.parse(again.stdout);
    assert.equal(empty.outcome, "clean");
    assert.deepEqual(empty.plan.batches, []);
    assert.deepEqual(empty.receipts, []);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

scenario("stale plan spans are re-analyzed before each batch", () => {
  const directory = makeRepo("stale-spans");
  try {
    write(directory, "src/index.ts", [
      'import { BARREL_MARKER } from "./barrel.ts";',
      "export const value = BARREL_MARKER;",
      "",
    ].join("\n"));
    // The first re-export line is deliberately longer than the rest of the
    // file. Removing it in batch 1 moves every later span. Applying batch 2
    // with plan-time offsets either throws `invalid span` or falls back to the
    // old line number and removes the marker line below.
    write(directory, "src/barrel.ts", [
      'export { deadA } from "./a-very-long-declaration-file-name.ts";',
      'export { deadB } from "./z.ts";',
      "export const BARREL_MARKER = 1;",
      "",
    ].join("\n"));
    write(directory, "src/a-very-long-declaration-file-name.ts", "export function deadA() { return 1; }\n");
    write(directory, "src/z.ts", "export function deadB() { return 2; }\n");
    write(directory, "package.json", `${JSON.stringify({
      name: "clean-all-stale-spans",
      private: true,
      type: "module",
      main: "./src/index.ts",
      exports: { ".": "./src/index.ts" },
    }, null, 2)}\n`);
    write(directory, "shrike.config.json", `${JSON.stringify({
      batchPolicy: { maxSymbols: 2, maxFiles: 2, maxLiveGraphPercent: 5, allowOversized: false },
      checks: [{
        name: "barrel-marker",
        command: "node -e \"const fs=require('fs');const b=fs.readFileSync('src/barrel.ts','utf8');process.exit(/^export const BARREL_MARKER = 1;$/m.test(b)?0:1)\"",
        required: true,
        timeoutMs: 30_000,
      }],
    }, null, 2)}\n`);
    git(directory, ["add", "-A"]);
    git(directory, ["commit", "-q", "-m", "fixture"]);

    const dry = run(directory, ["clean", "--all", "--dry-run", "--json"]);
    assert.equal(dry.status, 1, `dry-run exit: ${dry.stderr}`);
    const planned = JSON.parse(dry.stdout);
    const batchOf = new Map();
    for (const batch of planned.batches) {
      for (const symbol of batch.symbols) batchOf.set(symbol, batch.id);
    }
    assert.equal(planned.batches.length, 2, `unexpected plan: ${JSON.stringify(planned.batches)}`);
    assert.notEqual(
      batchOf.get("src/barrel.ts#deadA"),
      batchOf.get("src/barrel.ts#deadB"),
      "barrel aliases were not split across batches",
    );

    const result = run(directory, ["clean", "--all", "--in-place", "--json"]);
    assert.equal(result.status, 0, `apply exit: ${result.stderr}`);
    const output = JSON.parse(result.stdout);
    assert.equal(output.outcome, "verified");
    assert.equal(read(directory, "src/barrel.ts"), "export const BARREL_MARKER = 1;\n");
    assert.equal(read(directory, "src/index.ts").includes("BARREL_MARKER"), true, "entry was damaged");
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

scenario("worktree cleanup reports a deterministic relative path and leaves no worktree", () => {
  const directory = makeRepo("worktree");
  try {
    fixture(directory);

    const result = run(directory, ["clean", "--all", "--json"]);
    assert.equal(result.status, 0, `apply exit: ${result.stderr}`);
    const output = JSON.parse(result.stdout);
    assert.equal(output.scope, "current");
    assert.equal(output.outcome, "verified");
    assert.match(output.worktree, /^\.shrike\/worktrees\/shrike-cleanup-[A-Za-z0-9]+$/);
    assert.equal(output.worktree.includes(directory), false, "worktree path is absolute");
    assert.ok(existsSync(join(directory, output.receipts[0])), "receipt file is missing");

    const branch = output.branch;
    assert.match(
      git(directory, ["show", `${branch}:src/mixed.ts`]),
      /kept/,
      "live sibling was not committed",
    );
    assert.doesNotMatch(
      git(directory, ["show", `${branch}:src/mixed.ts`]),
      /deadExport/,
      "dead symbol was not committed",
    );
    assert.throws(
      () => git(directory, ["cat-file", "-e", `${branch}:src/orphan.ts`]),
      "orphan file still exists on the cleanup branch",
    );
    assert.match(git(directory, ["show", `${branch}:src/public.ts`]), /publicApi/);
    assert.match(git(directory, ["show", `${branch}:package.json`]), /left-pad/);

    const worktrees = git(directory, ["worktree", "list", "--porcelain"])
      .split("\n")
      .filter((line) => line.startsWith("worktree "));
    assert.equal(worktrees.length, 1, `temporary worktree leaked: ${worktrees.join(", ")}`);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

scenario("current cleanup without checks exits 4 and deletes nothing", () => {
  const directory = makeRepo("no-checks");
  try {
    fixture(directory, { checks: false });

    const result = run(directory, ["clean", "--all", "--in-place", "--json"]);
    assert.equal(result.status, 4, `expected exit 4, got ${result.status}: ${result.stderr}`);
    assert.equal(existsSync(join(directory, "src/orphan.ts")), true, "file was deleted without checks");
    assert.equal(read(directory, "src/mixed.ts").includes("deadExport"), true, "symbol was deleted without checks");
    assert.equal(existsSync(join(directory, ".shrike", "receipts")), false, "a receipt was written without checks");
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
