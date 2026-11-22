#!/usr/bin/env node
/** Set the release version across the workspace, Cargo, and the CLI banner. */
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const version = process.argv[2];
if (!version || !/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version)) {
  console.error("usage: node scripts/set-version.mjs <semver>");
  process.exit(1);
}

const manifests = [
  "package.json",
  "packages/cli/package.json",
  "packages/action/package.json",
  "packages/agent/package.json",
  "packages/receipts/package.json",
  "packages/testkit/package.json",
  "packages/verifier/package.json",
  "packages/platform/linux-x64/package.json",
  "packages/platform/linux-arm64/package.json",
  "packages/platform/darwin-x64/package.json",
  "packages/platform/darwin-arm64/package.json",
  "packages/platform/win32-x64/package.json",
];

for (const relative of manifests) {
  const path = join(root, relative);
  const manifest = JSON.parse(readFileSync(path, "utf8"));
  manifest.version = version;
  writeFileSync(path, `${JSON.stringify(manifest, null, 2)}\n`);
}

const versionPath = join(root, "packages", "cli", "src", "version.ts");
const versionSource = readFileSync(versionPath, "utf8");
const versionMatch = /export const VERSION = "([^"]+)";/.exec(versionSource);
if (!versionMatch) {
  console.error(`FAIL version: could not update ${versionPath}`);
  process.exit(1);
}
if (versionMatch[1] !== version) {
  writeFileSync(
    versionPath,
    versionSource.replace(
      /export const VERSION = "[^"]+";/,
      `export const VERSION = "${version}";`,
    ),
  );
}

const cargoPath = join(root, "Cargo.toml");
const cargo = readFileSync(cargoPath, "utf8");
const cargoMatch = /(\[workspace\.package\][\s\S]*?\nversion = ")([^"]+)(")/.exec(cargo);
if (!cargoMatch) {
  console.error(`FAIL version: could not update [workspace.package] in ${cargoPath}`);
  process.exit(1);
}
if (cargoMatch[2] !== version) {
  writeFileSync(
    cargoPath,
    cargo.replace(cargoMatch[0], () => `${cargoMatch[1]}${version}${cargoMatch[3]}`),
  );
}

console.log(`OK: set version ${version} in ${manifests.length} manifests, version.ts, and Cargo.toml`);
