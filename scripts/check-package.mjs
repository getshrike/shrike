#!/usr/bin/env node
/**
 * Local npm packaging gate.
 *
 * Verifies the CLI tarball contains the launcher, compiled host, README, and
 * license, and no source, tests, or fixtures. Registry publishing and signed
 * platform binaries remain release gates.
 */
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";

const root = resolve(new URL("..", import.meta.url).pathname);
const cli = join(root, "packages", "cli");
const dist = join(cli, "dist", "main.js");

if (!existsSync(dist)) {
  console.error("FAIL package: packages/cli/dist/main.js is missing; run `pnpm run build:ts`");
  process.exit(1);
}

const packed = spawnSync("npm", ["pack", "--dry-run", "--json"], {
  cwd: cli,
  encoding: "utf8",
  maxBuffer: 32 * 1024 * 1024,
});
if (packed.status !== 0) {
  console.error(`FAIL package: npm pack --dry-run exited ${packed.status}`);
  console.error(packed.stderr.trim());
  process.exit(1);
}

let result;
try {
  result = JSON.parse(packed.stdout)[0];
} catch {
  console.error("FAIL package: npm pack did not return JSON");
  process.exit(1);
}

const paths = new Set((result.files ?? []).map((file) => file.path));
const required = [
  "bin/shrike.mjs",
  "dist/main.js",
  "dist/index.js",
  "README.md",
  "LICENSE",
  "package.json",
];
const forbidden = [...paths].filter(
  (path) =>
    path.startsWith("src/") ||
    path.startsWith("test/") ||
    path.startsWith("fixtures/") ||
    (path.endsWith(".ts") && !path.endsWith(".d.ts")),
);
const missing = required.filter((path) => !paths.has(path));

const manifest = JSON.parse(readFileSync(join(cli, "package.json"), "utf8"));
if (manifest.bin?.shrike !== "./bin/shrike.mjs") {
  console.error("FAIL package: package.json bin does not point at bin/shrike.mjs");
  process.exit(1);
}

if (missing.length > 0 || forbidden.length > 0) {
  if (missing.length > 0) console.error(`FAIL package: missing ${missing.join(", ")}`);
  if (forbidden.length > 0) console.error(`FAIL package: forbidden files ${forbidden.join(", ")}`);
  process.exit(1);
}

console.log(
  `OK: shrike@${result.version} tarball ${result.filename} (${paths.size} files, ${result.size} bytes); ` +
    "launcher, dist, README, and license present; no source/tests/fixtures",
);
