#!/usr/bin/env node
/**
 * Run the native engine over the must-not-report corpus and assert that every
 * protected symbol is absent from `candidates`. This is the false-positive gate.
 *
 * Each corpus case is analyzed as its own project: manifest symbols are
 * case-relative, so a single root analysis would never match them.
 *
 * Every case runs twice:
 *   1. static analysis only;
 *   2. with synthetic build evidence that names every protected file/symbol.
 *
 * Build evidence may only add liveness, so the second run must keep the case
 * green and must report how many candidates the evidence suppressed.
 */
import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const root = resolve(new URL("..", import.meta.url).pathname);
const engine = process.env.SHRIKE_ENGINE ?? join(root, "target", "debug", "shrike-engine");
if (!existsSync(engine)) {
  console.error(`SKIP: engine not built at ${engine}`);
  process.exit(4);
}
const corpus = join(root, "fixtures", "must-not-report");
const manifest = JSON.parse(readFileSync(join(corpus, "manifest.json"), "utf8"));
// Analyze from a clean cache every run. A stale or shared cache must never be
// able to hide a resolution or reachability regression.
const cacheRoot = mkdtempSync(join(tmpdir(), "shrike-corpus-"));

function request(command, payload) {
  return new Promise((resolveRequest, reject) => {
    const child = spawn(engine, [], { stdio: ["pipe", "pipe", "pipe"] });
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => { stdout += chunk; });
    child.stderr.on("data", (chunk) => { stderr += chunk; });
    child.on("error", reject);
    child.on("close", (code) => {
      const line = stdout.split("\n").find((value) => value.trim());
      if (!line) return reject(new Error(`engine exit ${code}: ${stderr.trim()}`));
      const response = JSON.parse(line);
      if (!response.ok) return reject(new Error(`${response.error.code}: ${response.error.message}`));
      resolveRequest(response.result);
    });
    child.stdin.end(`${JSON.stringify({ protocolVersion: "1.0", id: "corpus", command, payload })}\n`);
  });
}

const failures = [];
let candidateTotal = 0;
let suppressedTotal = 0;

for (const entry of manifest.entries) {
  const directory = join(corpus, "cases", entry.id);
  let analyzed;
  try {
    analyzed = await request("analyze", {
      root: directory,
      config: {},
      cacheDir: join(cacheRoot, entry.id),
    });
  } catch (error) {
    failures.push(`${entry.id}: analysis failed: ${error instanceof Error ? error.message : String(error)}`);
    continue;
  }
  const snapshot = analyzed.snapshot;
  const keys = new Set(snapshot.symbols.map((symbol) => symbol.key));
  const candidates = new Set(snapshot.candidates);
  candidateTotal += candidates.size;

  const protectedItems = [];
  for (const item of entry.protected) {
    if (!keys.has(item.symbol)) {
      failures.push(`${entry.id}: protected symbol missing from snapshot: ${item.symbol}`);
      continue;
    }
    protectedItems.push(item);
    if (candidates.has(item.symbol)) {
      failures.push(`${entry.id}: ${item.symbol} (${item.reason})`);
    }
  }
  if (protectedItems.length === 0) continue;

  const files = [...new Set(protectedItems.map((item) => item.symbol.split("#")[0]).filter(Boolean))].sort();
  const symbols = [...new Set(protectedItems.map((item) => item.symbol))].sort();
  const augmented = await request("augment-build", {
    snapshot,
    build: {
      source: "corpus synthetic build evidence",
      version: "16.4.0",
      files,
      symbols,
      routes: [],
      sizes: {},
    },
  });
  if (!["full", "partial"].includes(augmented.join.status)) {
    failures.push(
      `${entry.id}: build join status ${augmented.join.status}; unmatched ${augmented.join.unmatched.join(", ")}`,
    );
    continue;
  }
  const buildCandidates = new Set(augmented.snapshot.candidates);
  for (const item of protectedItems) {
    if (buildCandidates.has(item.symbol)) {
      failures.push(`${entry.id}: protected symbol survived build evidence: ${item.symbol}`);
    }
  }
  suppressedTotal += [...candidates].filter((candidate) => !buildCandidates.has(candidate)).length;
}

rmSync(cacheRoot, { recursive: true, force: true });

if (failures.length > 0) {
  console.error(`FAIL: ${failures.length} corpus violations`);
  for (const failure of failures.slice(0, 40)) console.error(`  ${failure}`);
  process.exit(1);
}
console.log(
  `OK: ${manifest.entries.length} corpus cases; 0 protected symbols in ${candidateTotal} candidates; ` +
    `build evidence suppressed ${suppressedTotal} candidates`,
);
