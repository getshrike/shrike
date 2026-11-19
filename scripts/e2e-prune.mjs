#!/usr/bin/env node
/**
 * End-to-end prune gate.
 *
 * Builds throwaway git repositories and drives the real CLI and engine through:
 *   1. a verified batch (branch worktree, commit, receipt, cleanup)
 *   2. a rejected batch (baseline/after comparison, restore, rejected receipt)
 *
 * Requires target/debug/shrike-engine or SHRIKE_ENGINE.
 */
import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const cli = join(root, "packages", "cli", "src", "main.ts");
const engine = process.env.SHRIKE_ENGINE ?? join(root, "target", "debug", "shrike-engine");
if (!existsSync(engine)) {
  console.error(`SKIP e2e-prune: engine not found at ${engine}`);
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

function makeRepo(name) {
  const directory = mkdtempSync(join(tmpdir(), `shrike-e2e-${name}-`));
  git(directory, ["init", "-q", "-b", "main"]);
  git(directory, ["config", "user.email", "e2e@shrike.local"]);
  git(directory, ["config", "user.name", "Shrike E2E"]);
  return directory;
}

function baseFiles(directory, checkSource) {
  write(directory, "app/page.tsx", `import { formatInvoice } from "../lib/format";\nexport default function Page() { return formatInvoice(1); }\n`);
  write(directory, "lib/format.ts", `export const KEEP = "keep";\nexport function formatInvoice(value: number) { return String(value); }\n`);
  write(directory, "package.json", `{"name":"prune-e2e","private":true,"type":"module"}\n`);
  write(directory, "tsconfig.json", `{"compilerOptions":{"baseUrl":".","paths":{"@/*":["src/*"]}}}\n`);
  write(directory, "check.mjs", checkSource);
  write(directory, "shrike.config.json", JSON.stringify({
    checks: [{ name: "smoke", command: "node check.mjs", required: true, timeoutMs: 30_000 }],
  }));
}

function runPrune(directory, env = {}) {
  return spawnSync(process.execPath, [cli, "prune", "--base", "main", "--json"], {
    cwd: directory,
    encoding: "utf8",
    env: { ...process.env, SHRIKE_ENGINE: engine, ...env },
  });
}

function runCli(directory, args, env = {}) {
  return spawnSync(process.execPath, [cli, ...args], {
    cwd: directory,
    encoding: "utf8",
    env: { ...process.env, SHRIKE_ENGINE: engine, ...env },
  });
}

function receipts(directory) {
  const path = join(directory, ".shrike", "receipts");
  if (!existsSync(path)) return [];
  return readdirSync(path).filter((name) => name.startsWith("rcpt_")).map((name) => JSON.parse(readFileSync(join(path, name), "utf8")));
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

const failures = [];
function scenario(name, fn) {
  try {
    fn();
    console.log(`PASS ${name}`);
  } catch (error) {
    failures.push(name);
    console.error(`FAIL ${name}: ${error instanceof Error ? error.message : String(error)}`);
  }
}

scenario("verified batch", () => {
  const directory = makeRepo("verified");
  try {
    baseFiles(directory, `import { readFileSync } from "node:fs";\nconst page = readFileSync("app/page.tsx", "utf8");\nif (page.includes("formatInvoice")) throw new Error("stale caller");\n`);
    git(directory, ["add", "-A"]);
    git(directory, ["commit", "-q", "-m", "base"]);
    git(directory, ["checkout", "-q", "-b", "feature"]);
    write(directory, "app/page.tsx", `export default function Page() { return null; }\n`);
    git(directory, ["add", "-A"]);
    git(directory, ["commit", "-q", "-m", "remove caller"]);

    const delta = runCli(directory, ["delta", "--base", "main", "--json"]);
    assert(delta.status === 1, `expected delta exit 1, got ${delta.status}: ${delta.stderr}`);
    const deltaReport = JSON.parse(delta.stdout);
    assert(
      deltaReport.newlyDead.some((item) => item.symbol === "lib/format.ts#formatInvoice"),
      "delta did not report the newly dead symbol",
    );

    const result = runPrune(directory);
    assert(result.status === 0, `expected exit 0, got ${result.status}: ${result.stderr}`);
    const output = JSON.parse(result.stdout);
    assert(output.outcome === "verified", `expected verified, got ${output.outcome}`);
    const written = receipts(directory);
    assert(written.length === 1 && written[0].outcome === "verified", "expected one verified receipt");
    assert(written[0].batch.symbols.includes("lib/format.ts#formatInvoice"), "receipt missing deleted symbol");
    assert(written[0].checks[0].classification === "pass", "receipt check did not pass");
    const branches = git(directory, ["branch", "--list", "shrike/cleanup-*"]);
    assert(branches.length > 0, "cleanup branch was not created");
    const committed = git(directory, ["show", "shrike/cleanup-" + output.plan.id.replace(/^plan_/, "") + ":lib/format.ts"]);
    assert(!committed.includes("formatInvoice"), "deletion was not committed to the cleanup branch");
    const worktrees = git(directory, ["worktree", "list", "--porcelain"]).split("\n").filter((line) => line.startsWith("worktree "));
    assert(worktrees.length === 1, `temporary worktree leaked: ${worktrees.join(", ")}`);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

scenario("rejected batch", () => {
  const directory = makeRepo("rejected");
  try {
    baseFiles(directory, `import { readFileSync } from "node:fs";\nconst source = readFileSync("lib/format.ts", "utf8");\nif (!source.includes("formatInvoice")) throw new Error("dynamic consumer requires formatInvoice");\n`);
    git(directory, ["add", "-A"]);
    git(directory, ["commit", "-q", "-m", "base"]);
    git(directory, ["checkout", "-q", "-b", "feature"]);
    write(directory, "app/page.tsx", `export default function Page() { return null; }\n`);
    git(directory, ["add", "-A"]);
    git(directory, ["commit", "-q", "-m", "remove caller"]);

    const result = runPrune(directory);
    assert(result.status === 3, `expected exit 3, got ${result.status}: ${result.stderr}`);
    const output = JSON.parse(result.stdout);
    assert(output.outcome === "rejected", `expected rejected, got ${output.outcome}`);
    const written = receipts(directory);
    assert(written.length === 1 && written[0].outcome === "rejected", "expected one rejected receipt");
    assert(written[0].checks[0].classification === "fail", "rejected receipt did not record the failure");
    const restored = readFileSync(join(directory, "lib/format.ts"), "utf8");
    assert(restored.includes("formatInvoice"), "working tree was not restored");
    const dirty = git(directory, ["status", "--porcelain"]).split("\n").filter((line) => line.trim() && !line.slice(3).startsWith(".shrike/"));
    assert(dirty.length === 0, `tree is dirty after rejection: ${dirty.join(", ")}`);
    const worktrees = git(directory, ["worktree", "list", "--porcelain"]).split("\n").filter((line) => line.startsWith("worktree "));
    assert(worktrees.length === 1, `temporary worktree leaked: ${worktrees.join(", ")}`);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

scenario("bisected batch", () => {
  const directory = makeRepo("bisect");
  try {
    write(directory, "app/page.tsx", `import { good } from "../lib/good";\nimport { bad } from "../lib/bad";\nexport default function Page() { return good() + bad(); }\n`);
    write(directory, "lib/good.ts", `export function good() { return "good"; }\n`);
    write(directory, "lib/bad.ts", `export function bad() { return "bad"; }\n`);
    write(directory, "package.json", `{"name":"prune-bisect","private":true,"type":"module"}\n`);
    write(directory, "tsconfig.json", `{"compilerOptions":{"baseUrl":".","paths":{"@/*":["src/*"]}}}\n`);
    write(directory, "check.mjs", `import { readFileSync } from "node:fs";\nconst source = readFileSync("lib/bad.ts", "utf8");\nif (!source.includes("bad")) throw new Error("dynamic consumer requires bad");\n`);
    write(directory, "shrike.config.json", JSON.stringify({
      checks: [{ name: "smoke", command: "node check.mjs", required: true, timeoutMs: 30_000 }],
    }));
    git(directory, ["add", "-A"]);
    git(directory, ["commit", "-q", "-m", "base"]);
    git(directory, ["checkout", "-q", "-b", "feature"]);
    write(directory, "app/page.tsx", `export default function Page() { return null; }\n`);
    git(directory, ["add", "-A"]);
    git(directory, ["commit", "-q", "-m", "remove callers"]);

    const result = runPrune(directory);
    assert(result.status === 3, `expected exit 3, got ${result.status}: ${result.stderr}`);
    const output = JSON.parse(result.stdout);
    assert(output.outcome === "rejected", `expected a rejected remainder, got ${output.outcome}`);
    const written = receipts(directory);
    assert(written.some((receipt) => receipt.outcome === "rejected"), "missing rejected receipt");
    assert(written.some((receipt) => receipt.outcome === "verified"), "missing verified remainder receipt");
    const rejected = written.find((receipt) => receipt.outcome === "rejected");
    assert(rejected.rejected.length === 1, "bisection did not record one rejection");
    assert(rejected.rejected[0].symbols.includes("lib/bad.ts#bad"), "bisection rejected the wrong symbol");
    const branch = "shrike/cleanup-" + output.plan.id.replace(/^plan_/, "");
    const branchFiles = git(directory, ["ls-tree", "-r", "--name-only", branch]).split("\n");
    assert(!branchFiles.includes("lib/good.ts"), "passing remainder was not deleted");
    const bad = git(directory, ["show", `${branch}:lib/bad.ts`]);
    assert(bad.includes("bad"), "culprit was committed despite the rejection");
    const dirty = git(directory, ["status", "--porcelain"]).split("\n").filter((line) => line.trim() && !line.slice(3).startsWith(".shrike/"));
    assert(dirty.length === 0, `tree is dirty after bisection: ${dirty.join(", ")}`);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

scenario("revert failure and recovery", () => {
  const directory = makeRepo("revert");
  try {
    baseFiles(directory, `import { readFileSync } from "node:fs";\nconst page = readFileSync("app/page.tsx", "utf8");\nif (page.includes("formatInvoice")) throw new Error("stale caller");\n`);
    git(directory, ["add", "-A"]);
    git(directory, ["commit", "-q", "-m", "base"]);
    git(directory, ["checkout", "-q", "-b", "feature"]);
    write(directory, "app/page.tsx", `export default function Page() { return null; }\n`);
    git(directory, ["add", "-A"]);
    git(directory, ["commit", "-q", "-m", "remove caller"]);

    const result = runPrune(directory, { SHRIKE_TEST_FAIL_REVERT: "1" });
    assert(result.status === 5, `expected exit 5, got ${result.status}: ${result.stderr}`);
    const output = JSON.parse(result.stdout);
    assert(output.outcome === "revert-failed", `expected revert-failed, got ${output.outcome}`);
    assert(output.recovery.snapshotPath.startsWith(".shrike/recovery/rec_"), "missing recovery snapshot path");
    assert(output.recovery.restoreCommand.startsWith("shrike recover "), "missing restore command");

    const written = receipts(directory);
    assert(written.length === 1, "expected one revert-failed receipt");
    assert(written[0].outcome === "revert-failed", "receipt outcome was not revert-failed");
    assert(written[0].recovery.snapshotPath === output.recovery.snapshotPath, "receipt recovery does not match");
    assert(written[0].checks.length === 0, "revert before verification should not record checks");

    const worktrees = git(directory, ["worktree", "list", "--porcelain"]).split("\n").filter((line) => line.startsWith("worktree "));
    assert(worktrees.length === 1, `temporary worktree leaked: ${worktrees.join(", ")}`);

    // Simulate a tree left in an unknown state, then prove `shrike recover` restores it.
    writeFileSync(join(directory, "lib", "format.ts"), `export const KEEP = "keep";\n`);
    const recovered = runCli(directory, ["recover", output.recovery.snapshotPath, "--json"]);
    assert(recovered.status === 0, `recover failed: ${recovered.stderr}`);
    const recovery = JSON.parse(recovered.stdout);
    assert(recovery.restored.includes("lib/format.ts"), "recover did not restore the damaged file");
    const restored = readFileSync(join(directory, "lib", "format.ts"), "utf8");
    assert(restored.includes("formatInvoice"), "recover did not restore the original bytes");
    const dirty = git(directory, ["status", "--porcelain"]).split("\n").filter((line) => line.trim() && !line.slice(3).startsWith(".shrike/"));
    assert(dirty.length === 0, `tree is dirty after recovery: ${dirty.join(", ")}`);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

scenario("cascade deletion", () => {
  const directory = makeRepo("cascade");
  try {
    write(directory, "app/page.tsx", `import { outer } from "../lib/outer";\nexport default function Page() { return outer(); }\n`);
    write(directory, "lib/inner.ts", `export function inner() { return "inner"; }\n`);
    write(directory, "lib/outer.ts", `import { inner } from "./inner";\nexport function outer() { return inner(); }\n`);
    write(directory, "package.json", `{"name":"prune-cascade","private":true,"type":"module"}\n`);
    write(directory, "check.mjs", `import { existsSync, readFileSync } from "node:fs";\nfor (const [file, name] of [["lib/inner.ts", "inner"], ["lib/outer.ts", "outer"]]) {\n  if (!existsSync(file)) continue;\n  const source = readFileSync(file, "utf8");\n  if (!source.includes("function " + name)) throw new Error("deleted " + name + " left " + file);\n}\n`);
    write(directory, "shrike.config.json", JSON.stringify({
      checks: [{ name: "smoke", command: "node check.mjs", required: true, timeoutMs: 30_000 }],
    }));
    git(directory, ["add", "-A"]);
    git(directory, ["commit", "-q", "-m", "base"]);
    git(directory, ["checkout", "-q", "-b", "feature"]);
    write(directory, "app/page.tsx", `export default function Page() { return null; }\n`);
    git(directory, ["add", "-A"]);
    git(directory, ["commit", "-q", "-m", "remove caller"]);

    const delta = runCli(directory, ["delta", "--base", "main", "--json"]);
    assert(delta.status === 1, `expected delta exit 1, got ${delta.status}: ${delta.stderr}`);
    const symbols = JSON.parse(delta.stdout).newlyDead.map((item) => item.symbol);
    assert(symbols.includes("lib/outer.ts#outer"), "outer was not newly dead");
    assert(symbols.includes("lib/inner.ts#inner"), "inner was not cascaded");

    const result = runPrune(directory);
    assert(result.status === 0, `expected exit 0, got ${result.status}: ${result.stderr}`);
    const output = JSON.parse(result.stdout);
    assert(output.outcome === "verified", "cascade batch was not verified");
    const branch = "shrike/cleanup-" + output.plan.id.replace(/^plan_/, "");
    const branchFiles = git(directory, ["ls-tree", "-r", "--name-only", branch]).split("\n");
    assert(!branchFiles.includes("lib/outer.ts"), "outer file was left behind");
    assert(!branchFiles.includes("lib/inner.ts"), "inner file was left behind");
    const written = receipts(directory);
    assert(written.length === 1 && written[0].outcome === "verified", "expected one verified receipt");
    assert(written[0].batch.symbols.includes("lib/outer.ts#outer"), "receipt missing outer");
    assert(written[0].batch.symbols.includes("lib/inner.ts#inner"), "receipt missing inner");
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

if (failures.length > 0) {
  console.error(`FAIL: ${failures.length} prune scenario(s) failed`);
  process.exit(1);
}
console.log("OK: prune e2e (verified + rejected + revert recovery)");
