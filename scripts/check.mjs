#!/usr/bin/env node
/** Repository check: schemas, corpus, fixtures, and engine false-positive gate. */
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { join, resolve } from "node:path";

const root = resolve(new URL("..", import.meta.url).pathname);
const steps = [
  ["schemas", ["scripts/check-schemas.mjs"]],
  ["corpus", ["scripts/check-corpus.mjs"]],
  ["fixtures", ["scripts/run-fixtures.mjs", "--engine"]],
  ["corpus-engine", ["scripts/corpus-runner.mjs"]],
];
let failed = false;
for (const [name, args] of steps) {
  if (name.includes("engine") && !existsSync(join(root, "target", "debug", "shrike-engine"))) {
    console.log(`SKIP ${name}: build shrike-engine first`);
    continue;
  }
  const result = spawnSync(process.execPath, args, { cwd: root, stdio: "inherit" });
  if (result.status !== 0) failed = true;
}
process.exit(failed ? 1 : 0);
