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

function runCli(args, env = {}, cwd = temp) {
  return spawnSync(
    process.execPath,
    [join(temp, "node_modules", "getshrike", "bin", "shrike.mjs"), ...args],
    {
      cwd,
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

  const mcpRequests = [
    { jsonrpc: "2.0", id: 1, method: "initialize", params: {} },
    { jsonrpc: "2.0", id: 2, method: "tools/list" },
    { jsonrpc: "2.0", id: 3, method: "tools/call", params: { name: "shrike_receipt", arguments: {} } },
  ]
    .map((request) => JSON.stringify(request))
    .join("\n");
  const mcp = spawnSync(
    process.execPath,
    [join(installed, "bin", "shrike.mjs"), "mcp", "serve"],
    {
      cwd: temp,
      encoding: "utf8",
      input: `${mcpRequests}\n`,
      maxBuffer: 64 * 1024 * 1024,
    },
  );
  if (mcp.status !== 0) fail("installed mcp serve failed", `${mcp.stdout}\n${mcp.stderr}`);
  const mcpResponses = mcp.stdout
    .trim()
    .split("\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line));
  const receiptCall = mcpResponses.find((response) => response.id === 3);
  if (receiptCall?.result?.isError !== false) {
    fail("installed mcp serve could not run a read-only tool", mcp.stdout);
  }
  if (!/receipts/.test(receiptCall.result.content?.[0]?.text ?? "")) {
    fail("installed mcp serve returned no receipt payload", mcp.stdout);
  }

  const skillFiles = [
    ".agents/skills/shrike/SKILL.md",
    ".agents/skills/shrike/references/commands.md",
    ".agents/skills/shrike/references/safety.md",
    ".agents/skills/shrike/references/setup.md",
    ".agents/skills/shrike/references/workflows.md",
  ];
  const dryInstall = runCli(["install", "--dry-run", "--json"]);
  if (dryInstall.status !== 0) {
    fail("shrike install --dry-run failed", `${dryInstall.stdout}\n${dryInstall.stderr}`);
  }
  const dry = JSON.parse(dryInstall.stdout);
  const dryPaths = dry.actions.map((action) => action.path).filter((path) => path.startsWith(".agents/skills/shrike/"));
  if (JSON.stringify(dryPaths) !== JSON.stringify(skillFiles)) {
    fail(`dry run did not list all five skill files: ${dryPaths.join(", ")}`);
  }
  if (dry.actions.some((action) => action.action !== "created")) {
    fail(`fresh install dry run did not report created: ${dryInstall.stdout.trim()}`);
  }
  if (/\/home\/|\/Users\/|\\Users\\|[A-Za-z]:\\/.test(dryInstall.stdout)) {
    fail("install output contains an absolute host path", dryInstall.stdout);
  }
  if (existsSync(join(temp, ".agents"))) fail("install --dry-run wrote the skill directory");

  const installedSkill = runCli(["install", "--json"]);
  if (installedSkill.status !== 0) {
    fail("shrike install failed", `${installedSkill.stdout}\n${installedSkill.stderr}`);
  }
  const installedResult = JSON.parse(installedSkill.stdout);
  const installedPaths = installedResult.actions.map((action) => action.path).filter((path) => path.startsWith(".agents/skills/shrike/"));
  if (JSON.stringify(installedPaths) !== JSON.stringify(skillFiles)) {
    fail(`install did not write all five skill files: ${installedPaths.join(", ")}`);
  }
  for (const path of skillFiles) {
    if (!existsSync(join(temp, path))) fail(`installed skill file is missing: ${path}`);
  }
  const skillText = readFileSync(join(temp, skillFiles[0]), "utf8");
  if (!skillText.startsWith("---\nname: shrike\n")) fail("installed SKILL.md has invalid frontmatter");
  if (!skillText.includes("license: MIT")) fail("installed SKILL.md is missing a license");
  if (/npx shrike/.test(skillText)) fail("installed SKILL.md contains the forbidden zero-install form");
  for (const reference of skillFiles.slice(1)) {
    const relative = reference.replace(".agents/skills/shrike/", "");
    if (!skillText.includes(`(${relative})`)) fail(`installed SKILL.md does not link ${relative}`);
    const text = readFileSync(join(temp, reference), "utf8");
    if (!text.endsWith("\n")) fail(`installed reference does not end with a newline: ${reference}`);
    if (/npx shrike/.test(text)) fail(`installed reference contains the forbidden zero-install form: ${reference}`);
  }

  const repeat = runCli(["install", "--dry-run", "--json"]);
  if (repeat.status !== 0) fail("repeated install dry run failed", repeat.stderr);
  if (JSON.parse(repeat.stdout).actions.some((action) => action.action !== "unchanged")) {
    fail("repeated install is not idempotent", repeat.stdout);
  }

  const scan = runCli(["scan", "--json"]);
  if (scan.status !== 0 && scan.status !== 1) {
    fail("scan with the installed engine failed", `${scan.stdout}\n${scan.stderr}`);
  }
  try {
    JSON.parse(scan.stdout);
  } catch {
    fail("scan did not return JSON", scan.stdout);
  }

  // Deterministic eval fixture: report and delta must run from a clean project
  // that has no Shrike config installed by the CLI. This covers the fixture the
  // manual skill evals use without depending on an agent.
  const evalFixture = join(temp, "eval-fixture");
  const setupFixture = spawnSync(
    process.execPath,
    [join(root, "evals", "skills", "shrike", "fixtures", "cleanup-app", "setup.mjs"), evalFixture],
    { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 },
  );
  if (setupFixture.status !== 0) {
    fail("eval fixture setup failed", `${setupFixture.stdout}\n${setupFixture.stderr}`);
  }
  const fixtureReport = runCli(["report", "--json"], {}, evalFixture);
  if (fixtureReport.status !== 0 && fixtureReport.status !== 1) {
    fail("report on the eval fixture failed", `${fixtureReport.stdout}\n${fixtureReport.stderr}`);
  }
  let fixtureData;
  try {
    fixtureData = JSON.parse(fixtureReport.stdout);
  } catch {
    fail("report on the eval fixture did not return JSON", fixtureReport.stdout);
  }
  if (!Number.isInteger(fixtureData.summary?.analyzedFiles) || fixtureData.summary.analyzedFiles < 5) {
    fail("report on the eval fixture analyzed too few files", fixtureReport.stdout);
  }
  const fixtureDelta = runCli(["delta", "--base", "main", "--json"], {}, evalFixture);
  if (fixtureDelta.status !== 0 && fixtureDelta.status !== 1) {
    fail("delta on the eval fixture failed", `${fixtureDelta.stdout}\n${fixtureDelta.stderr}`);
  }
  const deltaData = JSON.parse(fixtureDelta.stdout);
  if (
    !deltaData.newlyDead?.some((item) => item.symbol === "src/format.ts#formatInvoice")
  ) {
    fail("eval fixture delta did not report the branch-orphaned symbol", fixtureDelta.stdout);
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
