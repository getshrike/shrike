#!/usr/bin/env node
/** Lightweight lint gate: schema/corpus shape plus accidental console.log in libraries. */
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
const root = resolve(new URL("..", import.meta.url).pathname);
const check = spawnSync(process.execPath, ["scripts/check-schemas.mjs"], { cwd: root, stdio: "inherit" });
if (check.status !== 0) process.exit(check.status ?? 1);
const problems = [];
function walk(directory) {
  for (const name of readdirSync(directory)) {
    const path = join(directory, name);
    if (statSync(path).isDirectory()) walk(path);
    else if (name.endsWith(".ts") && !name.endsWith(".test.ts")) {
      const text = readFileSync(path, "utf8");
      if (/\bconsole\.log\(/.test(text)) problems.push(`${path}: console.log in a library`);
    }
  }
}
for (const pkg of ["verifier", "receipts", "agent", "testkit"]) walk(join(root, "packages", pkg, "src"));
if (problems.length > 0) { for (const p of problems) console.error(`FAIL ${p}`); process.exit(1); }
console.log("OK: lint checks");
