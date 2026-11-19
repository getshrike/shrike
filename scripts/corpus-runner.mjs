#!/usr/bin/env node
/**
 * Run the native engine over the must-not-report corpus and assert that every
 * protected symbol is absent from `candidates`. This is the false-positive gate.
 */
import { spawn } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";

const root = resolve(new URL("..", import.meta.url).pathname);
const engine = process.env.SHRIKE_ENGINE ?? join(root, "target", "debug", "shrike-engine");
if (!existsSync(engine)) {
  console.error(`SKIP: engine not built at ${engine}`);
  process.exit(4);
}
const manifest = JSON.parse(readFileSync(join(root, "fixtures", "must-not-report", "manifest.json"), "utf8"));

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

const result = await request("analyze", { root: join(root, "fixtures", "must-not-report"), config: {} });
const snapshot = result.snapshot;
const candidates = new Set(snapshot.candidates);
const failures = [];
for (const entry of manifest.entries) {
  for (const item of entry.protected) {
    if (candidates.has(item.symbol)) failures.push(`${entry.id}: ${item.symbol} (${item.reason})`);
  }
}
if (failures.length > 0) {
  console.error(`FAIL: ${failures.length} protected symbols were reported as candidates`);
  for (const failure of failures.slice(0, 40)) console.error(`  ${failure}`);
  process.exit(1);
}
console.log(`OK: ${manifest.entries.length} corpus cases; 0 protected symbols in ${candidates.size} candidates`);
