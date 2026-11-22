#!/usr/bin/env node
import { readFileSync, readdirSync } from "node:fs";
import { join, resolve } from "node:path";
const root = resolve(new URL("..", import.meta.url).pathname);
const docs = readdirSync(join(root, "docs")).filter((name) => name.endsWith(".md"));
const plans = readdirSync(join(root, "plans")).filter((name) => name.endsWith(".md"));
const tracker = readFileSync(join(root, "TRACKER.md"), "utf8");
const ids = new Set([...tracker.matchAll(/\| ([0-6]\.\d+) \|/g)].map((m) => m[1]));
if (docs.length < 8) { console.error(`FAIL: expected >= 8 docs, found ${docs.length}`); process.exit(1); }
if (plans.length < 7) { console.error(`FAIL: expected >= 7 plans, found ${plans.length}`); process.exit(1); }
if (ids.size < 50) { console.error(`FAIL: tracker task ids ${ids.size}`); process.exit(1); }
console.log(`OK: ${docs.length} docs, ${plans.length} plans, ${ids.size} tracker task ids`);
