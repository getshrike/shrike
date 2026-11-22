#!/usr/bin/env node
/** Build release-manifest.json from the files staged for a GitHub release. */
import { createHash } from "node:crypto";
import { readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const { values } = parseArgs({
  options: {
    version: { type: "string" },
    dir: { type: "string" },
    out: { type: "string" },
    commit: { type: "string" },
  },
});

if (!values.version || !values.dir) {
  console.error("usage: node scripts/release-manifest.mjs --version <version> --dir <dir> [--out <file>]");
  process.exit(1);
}

const directory = resolve(values.dir);
const files = readdirSync(directory)
  .filter((name) => {
    const path = join(directory, name);
    if (!statSync(path).isFile()) return false;
    return (
      name.endsWith(".tgz") ||
      name.startsWith("shrike-engine-") ||
      (name.endsWith(".sha256") && name.startsWith("shrike-engine-"))
    );
  })
  .sort();

const artifacts = files.map((name) => ({
  name,
  sha256: createHash("sha256").update(readFileSync(join(directory, name))).digest("hex"),
}));

const manifest = {
  name: "getshrike",
  version: values.version,
  commit: values.commit ?? process.env.GITHUB_SHA ?? null,
  generatedAt: new Date().toISOString(),
  npm: {
    getshrike: values.version,
    "getshrike-linux-x64": values.version,
    "getshrike-linux-arm64": values.version,
    "getshrike-darwin-x64": values.version,
    "getshrike-darwin-arm64": values.version,
    "getshrike-windows-x64": values.version,
  },
  artifacts,
};

const output = values.out ? resolve(values.out) : join(directory, "release-manifest.json");
writeFileSync(output, `${JSON.stringify(manifest, null, 2)}\n`);
console.log(`OK: wrote ${basename(output)} with ${artifacts.length} artifacts for getshrike@${values.version}`);
