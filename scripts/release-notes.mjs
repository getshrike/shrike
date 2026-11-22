#!/usr/bin/env node
/** Extract the release notes for one version from CHANGELOG.md. */
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const { values } = parseArgs({
  options: {
    version: { type: "string" },
    prerelease: { type: "boolean", default: false },
  },
});

if (!values.version) {
  console.error("usage: node scripts/release-notes.mjs --version <version> [--prerelease]");
  process.exit(1);
}

const changelog = readFileSync(join(root, "CHANGELOG.md"), "utf8");

function section(version) {
  const marker = `## [${version}]`;
  const start = changelog.indexOf(marker);
  if (start === -1) return null;
  const next = changelog.indexOf("\n## ", start + marker.length);
  return changelog.slice(start, next === -1 ? undefined : next).trim();
}

let body = section(values.version);
if (!body && values.version.includes("-")) body = section(values.version.split("-")[0]);
if (!body) {
  console.error(`FAIL release-notes: no CHANGELOG.md section for ${values.version}`);
  process.exit(1);
}
body = body.replace(/^## \[[^\]]+\][^\n]*\n+/, "").trim();
if (values.prerelease) {
  body =
    "> **Release candidate.** This is the 1.0.0 code line published for validation; " +
    "the final 1.0.0 release follows after external acceptance.\n\n" +
    body;
}
process.stdout.write(`${body}\n`);
