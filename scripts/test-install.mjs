#!/usr/bin/env node
/**
 * Clean-machine install test.
 *
 * Packs the CLI and the host platform package, installs them into an empty
 * project with scripts disabled, then verifies the launcher, the checksum
 * boundary, and the missing-platform failure mode.
 */
import { spawnSync } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  renameSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const platform = `${process.platform}-${process.arch}`;
const SUPPORTED = new Set(["linux-x64", "linux-arm64", "darwin-x64", "darwin-arm64", "win32-x64"]);
const PACKAGE_NAMES = { "win32-x64": "getshrike-windows-x64" };
if (!SUPPORTED.has(platform)) {
  console.log(`SKIP install test: unsupported host platform ${platform}`);
  process.exit(0);
}

const cli = join(root, "packages", "cli");
const platformDir = join(root, "packages", "platform", platform);
const packageName = PACKAGE_NAMES[platform] ?? `getshrike-${platform}`;
const engineName = process.platform === "win32" ? "shrike-engine.exe" : "shrike-engine";
const version = JSON.parse(readFileSync(join(cli, "package.json"), "utf8")).version;

function fail(message, output = "") {
  console.error(`FAIL install: ${message}`);
  if (output.trim()) console.error(output.trim());
  process.exit(1);
}

if (!existsSync(join(cli, "dist", "main.js"))) {
  fail("packages/cli/dist/main.js is missing; run `pnpm run build:ts`");
}
if (!existsSync(join(platformDir, "bin", engineName))) {
  fail(`packages/platform/${platform} is not staged; run scripts/stage-platform.mjs`);
}

const temp = mkdtempSync(join(tmpdir(), "shrike-install-"));
const keep = process.env.SHRIKE_TEST_KEEP === "1";
const packs = join(temp, "packs");
mkdirSync(packs);
writeFileSync(
  join(temp, "package.json"),
  `${JSON.stringify({ name: "shrike-install-test", version: "0.0.0", private: true }, null, 2)}\n`,
);

function pack(directory) {
  const destination = mkdtempSync(join(packs, "pkg-"));
  const result = spawnSync("pnpm", ["pack", "--pack-destination", destination], {
    cwd: directory,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  });
  if (result.status !== 0) fail(`pnpm pack failed in ${directory}`, result.stderr);
  const tarball = readdirSync(destination).find((name) => name.endsWith(".tgz"));
  if (!tarball) fail(`pnpm pack produced no tarball in ${destination}`);
  return join(destination, tarball);
}

function runCli(args, env = {}) {
  return spawnSync(
    process.execPath,
    [join(temp, "node_modules", "getshrike", "bin", "shrike.mjs"), ...args],
    {
      cwd: temp,
      encoding: "utf8",
      maxBuffer: 64 * 1024 * 1024,
      env: { ...process.env, ...env },
    },
  );
}

function expectFailure(label, args) {
  const result = runCli(args);
  if (result.status !== 4) {
    fail(`${label}: expected exit 4, got ${result.status}`, `${result.stdout}\n${result.stderr}`);
  }
  return `${result.stdout}\n${result.stderr}`;
}

try {
  const platformTarball = pack(platformDir);
  const cliTarball = pack(cli);
  const install = spawnSync(
    "npm",
    [
      "install",
      "--no-save",
      "--no-package-lock",
      "--ignore-scripts",
      "--omit=optional",
      "--no-audit",
      "--no-fund",
      platformTarball,
      cliTarball,
    ],
    { cwd: temp, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 },
  );
  if (install.status !== 0) fail("npm install failed", `${install.stdout}\n${install.stderr}`);

  const installed = join(temp, "node_modules", "getshrike");
  const installedManifest = JSON.parse(readFileSync(join(installed, "package.json"), "utf8"));
  if (installedManifest.name !== "getshrike") fail(`installed package is ${installedManifest.name}`);
  if (installedManifest.version !== version) {
    fail(`installed version ${installedManifest.version} does not match ${version}`);
  }
  if (installedManifest.dependencies && Object.keys(installedManifest.dependencies).length > 0) {
    fail("the installed package has runtime dependencies");
  }
  if (existsSync(join(temp, "node_modules", "@shrike")) || existsSync(join(installed, "node_modules", "@shrike"))) {
    fail("private @shrike packages were installed");
  }

  const direct = runCli(["version", "--json"]);
  if (direct.status !== 0) fail("shrike version failed", `${direct.stdout}\n${direct.stderr}`);
  const reported = JSON.parse(direct.stdout);
  if (reported.version !== version || reported.name !== "shrike") {
    fail(`unexpected version output: ${direct.stdout.trim()}`);
  }

  const npx = spawnSync("npx", ["--no-install", "getshrike", "version", "--json"], {
    cwd: temp,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  });
  if (npx.status !== 0) fail("npx getshrike failed", `${npx.stdout}\n${npx.stderr}`);
  if (JSON.parse(npx.stdout).version !== version) fail("npx getshrike reported the wrong version");

  const scan = runCli(["scan", "--json"]);
  if (scan.status !== 0 && scan.status !== 1) {
    fail("scan with the installed engine failed", `${scan.stdout}\n${scan.stderr}`);
  }
  try {
    JSON.parse(scan.stdout);
  } catch {
    fail("scan did not return JSON", scan.stdout);
  }

  const sidecar = join(temp, "node_modules", packageName, "bin", `${engineName}.sha256`);
  const originalSidecar = readFileSync(sidecar, "utf8");
  writeFileSync(sidecar, `${"0".repeat(64)}  ${engineName}\n`);
  const mismatch = expectFailure("checksum mismatch", ["scan", "--json"]);
  if (!/checksum mismatch/.test(mismatch)) fail("checksum mismatch did not name the failure", mismatch);
  writeFileSync(sidecar, originalSidecar);

  const platformInstall = join(temp, "node_modules", packageName);
  renameSync(platformInstall, `${platformInstall}.bak`);
  const missing = expectFailure("missing platform package", ["scan", "--json"]);
  if (!/was not found|not installed/.test(missing)) fail("missing platform package did not explain the failure", missing);
  renameSync(`${platformInstall}.bak`, platformInstall);

  console.log(
    `OK: clean-machine install getshrike@${version} on ${platform}; ` +
      "both bins work, checksum mismatch fails closed, missing platform package fails closed",
  );
} finally {
  if (keep) console.log(`Kept install test directory: ${temp}`);
  else rmSync(temp, { recursive: true, force: true });
}
