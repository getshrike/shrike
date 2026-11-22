#!/usr/bin/env node
/** Write a deterministic SHA256SUMS file for release artifacts. */
import { createHash } from "node:crypto";
import { readFileSync, statSync, writeFileSync } from "node:fs";
import { basename, resolve } from "node:path";
import { parseArgs } from "node:util";

const { values, positionals } = parseArgs({
  allowPositionals: true,
  options: { out: { type: "string" } },
});

const files = positionals
  .map((path) => resolve(path))
  .filter((path) => {
    if (!statSync(path).isFile()) return false;
    const name = basename(path);
    return name !== "SHA256SUMS" && !name.endsWith(".bundle") && !name.endsWith(".sig") && !name.endsWith(".pem");
  })
  .sort((left, right) => basename(left).localeCompare(basename(right)));

if (files.length === 0) {
  console.error("usage: node scripts/checksums.mjs [--out SHA256SUMS] <file>...");
  process.exit(1);
}

const lines = files.map(
  (path) => `${createHash("sha256").update(readFileSync(path)).digest("hex")}  ${basename(path)}`,
);
const output = `${lines.join("\n")}\n`;
if (values.out) writeFileSync(values.out, output);
else process.stdout.write(output);
