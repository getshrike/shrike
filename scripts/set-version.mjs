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
const nextVersionSource = versionSource.replace(
  /export const VERSION = "[^"]+";/,
  `export const VERSION = "${version}";`,
);
if (nextVersionSource === versionSource) {
  console.error(`FAIL version: could not update ${versionPath}`);
  process.exit(1);
}
writeFileSync(versionPath, nextVersionSource);

const cargoPath = join(root, "Cargo.toml");
const cargo = readFileSync(cargoPath, "utf8");
const nextCargo = cargo.replace(
  /(\[workspace\.package\][\s\S]*?\nversion = ")[^"]+(")/,
  `$1${version}$2`,
);
if (nextCargo === cargo) {
  console.error(`FAIL version: could not update [workspace.package] in ${cargoPath}`);
  process.exit(1);
}
writeFileSync(cargoPath, nextCargo);

console.log(`OK: set version ${version} in ${manifests.length} manifests, version.ts, and Cargo.toml`);
