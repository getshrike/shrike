#!/usr/bin/env node
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
const root = resolve(new URL("..", import.meta.url).pathname);
const failures = [];
let count = 0;
function walk(directory) {
  for (const name of readdirSync(directory)) {
    const path = join(directory, name);
    // Third-party and generated trees are not part of this repository's docs;
    // their READMEs contain links to files packages omit from publication.
    if (statSync(path).isDirectory()) {
      if (name === "node_modules" || name === "target" || name === "dist" || name === ".cloudflare") continue;
      walk(path);
    }
    else if (name.endsWith(".md")) {
      count += 1;
      const text = readFileSync(path, "utf8");
      for (const match of text.matchAll(/\]\(([^)]+)\)/g)) {
        const target = match[1];
        if (/^(https?:|mailto:|#)/.test(target)) continue;
        const clean = target.split("#")[0];
        if (!clean) continue;
        if (!existsSync(resolve(dirname(path), clean))) failures.push(`${path}: ${target}`);
      }
    }
  }
}
walk(root);
if (failures.length) { for (const f of failures) console.error(`FAIL ${f}`); process.exit(1); }
console.log(`OK: ${count} Markdown files; all relative links resolve`);
