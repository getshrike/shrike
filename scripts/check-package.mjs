#!/usr/bin/env node
/**
 * Packaging gate for the npm release.
 *
 * Packs the real tarball with pnpm (which rewrites `workspace:*`), then checks
 * the manifest, the file list, the bundle boundary, and a staged platform
 * package. `--platform <name>` checks one native package instead of the CLI.
 */
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const PLATFORM_PACKAGES = [
  "getshrike-linux-x64",
  "getshrike-linux-arm64",
  "getshrike-darwin-x64",
  "getshrike-darwin-arm64",
  "getshrike-windows-x64",
];
const PLATFORM_NAMES = { "win32-x64": "getshrike-windows-x64" };
const CLI_BUNDLES = new Set(["dist/main.js", "dist/index.js", "dist/config.js"]);

const { values } = parseArgs({ options: { platform: { type: "string" } } });

function fail(message) {
  console.error(`FAIL package: ${message}`);
  process.exit(1);
}

function run(command, args, cwd) {
  const result = spawnSync(command, args, { cwd, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
  if (result.error) fail(`${command} could not run: ${result.error.message}`);
  if (result.status !== 0) {
    fail(`${command} ${args.join(" ")} exited ${result.status}\n${(result.stderr ?? "").trim()}`);
  }
  return result.stdout ?? "";
}

function pack(directory) {
  const destination = mkdtempSync(join(tmpdir(), "shrike-pack-"));
  run("pnpm", ["pack", "--pack-destination", destination], directory);
  const tarballs = readdirSync(destination).filter((name) => name.endsWith(".tgz"));
  if (tarballs.length !== 1) fail(`expected one tarball in ${destination}, found ${tarballs.length}`);
  return { tarball: join(destination, tarballs[0]), cleanup: () => rmSync(destination, { recursive: true, force: true }) };
}

function tarList(tarball) {
  return run("tar", ["-tzf", tarball], root)
    .split("\n")
    .filter(Boolean)
    .map((path) => path.replace(/^package\//, ""));
}

function tarRead(tarball, path) {
  return run("tar", ["-xOzf", tarball, `package/${path}`], root);
}

function checkCommonFiles(paths, manifest) {
  const forbidden = [...paths].filter(
    (path) =>
      path.startsWith("src/") ||
      path.startsWith("test/") ||
      path.startsWith("fixtures/") ||
      path.startsWith("node_modules/") ||
      (path.endsWith(".ts") && !path.endsWith(".d.ts")),
  );
  if (forbidden.length > 0) fail(`forbidden files: ${forbidden.join(", ")}`);
  if (JSON.stringify(manifest).includes("workspace:")) {
    fail("manifest still contains a workspace: protocol");
  }
}

function checkPlatform(name) {
  const expectedName = PLATFORM_NAMES[name] ?? `getshrike-${name}`;
  const directory = join(root, "packages", "platform", name);
  const manifestPath = join(directory, "package.json");
  if (!existsSync(manifestPath)) fail(`platform manifest not found: ${manifestPath}`);
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  if (manifest.name !== expectedName) {
    fail(`${manifestPath} has name ${manifest.name}, expected ${expectedName}`);
  }
  const binaryName = manifest.os?.[0] === "win32" ? "shrike-engine.exe" : "shrike-engine";
  const binary = join(directory, "bin", binaryName);
  const sidecar = `${binary}.sha256`;
  if (!existsSync(binary)) fail(`${name} is not staged: ${binary} is missing`);
  if (!existsSync(sidecar)) fail(`${name} is not staged: ${sidecar} is missing`);
  const expected = readFileSync(sidecar, "utf8").trim().split(/\s+/)[0]?.toLowerCase() ?? "";
  if (!/^[0-9a-f]{64}$/.test(expected)) fail(`${name} has a malformed checksum sidecar`);
  const actual = createHash("sha256").update(readFileSync(binary)).digest("hex");
  if (actual !== expected) fail(`${name} checksum mismatch: expected ${expected}, got ${actual}`);

  const packed = pack(directory);
  try {
    const paths = new Set(tarList(packed.tarball));
    const packedManifest = JSON.parse(tarRead(packed.tarball, "package.json"));
    const required = [
      `bin/${binaryName}`,
      `bin/${binaryName}.sha256`,
      "README.md",
      "LICENSE",
      "package.json",
    ];
    const missing = required.filter((path) => !paths.has(path));
    if (missing.length > 0) fail(`${expectedName} tarball is missing ${missing.join(", ")}`);
    if (packedManifest.name !== expectedName) {
      fail(`${expectedName} tarball has name ${packedManifest.name}`);
    }
    if (packedManifest.version !== manifest.version) {
      fail(`${expectedName} tarball version ${packedManifest.version} does not match ${manifest.version}`);
    }
    if (packedManifest.os?.[0] !== manifest.os?.[0] || packedManifest.cpu?.[0] !== manifest.cpu?.[0]) {
      fail(`${expectedName} tarball os/cpu does not match the source manifest`);
    }
    checkCommonFiles(paths, packedManifest);
    console.log(
      `OK: ${expectedName}@${packedManifest.version} tarball (${paths.size} files, ` +
        `sha256 ${expected.slice(0, 12)}...); binary, sidecar, README, and license present`,
    );
  } finally {
    packed.cleanup();
  }
}

if (values.platform) {
  checkPlatform(values.platform);
  process.exit(0);
}

const cli = join(root, "packages", "cli");
for (const path of ["dist/main.js", "dist/index.js", "dist/config.js", "dist/index.d.ts", "dist/config.d.ts"]) {
  if (!existsSync(join(cli, path))) {
    fail(`packages/cli/${path} is missing; run \`pnpm run build:ts\``);
  }
}

const packed = pack(cli);
try {
  const paths = new Set(tarList(packed.tarball));
  const manifest = JSON.parse(tarRead(packed.tarball, "package.json"));
  const required = [
    "bin/shrike.mjs",
    "dist/main.js",
    "dist/index.js",
    "dist/config.js",
    "dist/index.d.ts",
    "dist/config.d.ts",
    "README.md",
    "LICENSE",
    "package.json",
  ];
  const missing = required.filter((path) => !paths.has(path));
  if (missing.length > 0) fail(`tarball is missing ${missing.join(", ")}`);

  if (manifest.name !== "getshrike") fail(`package name is ${manifest.name}, expected getshrike`);
  if (manifest.bin?.shrike !== "./bin/shrike.mjs" || manifest.bin?.getshrike !== "./bin/shrike.mjs") {
    fail("package.json must expose both shrike and getshrike bins at ./bin/shrike.mjs");
  }
  if (manifest.dependencies && Object.keys(manifest.dependencies).length > 0) {
    fail(`published package must not have runtime dependencies: ${Object.keys(manifest.dependencies).join(", ")}`);
  }
  const optional = manifest.optionalDependencies ?? {};
  const missingPlatforms = PLATFORM_PACKAGES.filter((name) => optional[name] !== manifest.version);
  if (missingPlatforms.length > 0) {
    fail(
      `optionalDependencies must pin ${PLATFORM_PACKAGES.join(", ")} to ${manifest.version}; ` +
        `missing or mismatched: ${missingPlatforms.join(", ")}`,
    );
  }
  if (manifest.engines?.node !== ">=22") fail(`engines.node is ${manifest.engines?.node}, expected >=22`);
  if (manifest.exports?.["./package.json"] !== "./package.json") {
    fail("package.json must export ./package.json for tooling that resolves the package root");
  }
  if (manifest.publishConfig?.provenance !== true) fail("publishConfig.provenance must be true");

  const unbundled = [...paths].filter(
    (path) => path.startsWith("dist/") && path.endsWith(".js") && !CLI_BUNDLES.has(path),
  );
  if (unbundled.length > 0) fail(`unbundled JavaScript in dist: ${unbundled.join(", ")}`);

  const main = tarRead(packed.tarball, "dist/main.js");
  if (/(?:from\s*["']@shrike\/|import\(\s*["']@shrike\/|require\(\s*["']@shrike\/)/.test(main)) {
    fail("dist/main.js still imports a private @shrike package");
  }
  if (!main.includes("getshrike-linux-x64")) fail("dist/main.js does not contain the platform package resolver");
  for (const declaration of ["dist/index.d.ts", "dist/config.d.ts"]) {
    const text = tarRead(packed.tarball, declaration);
    if (/@shrike\//.test(text)) fail(`${declaration} references a private @shrike package`);
  }

  checkCommonFiles(paths, manifest);

  const dryRun = spawnSync("pnpm", ["publish", "--dry-run", "--no-git-checks"], {
    cwd: cli,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  });
  if (dryRun.status !== 0) {
    fail(`pnpm publish --dry-run exited ${dryRun.status}\n${(dryRun.stderr ?? "").trim()}`);
  }

  console.log(
    `OK: getshrike@${manifest.version} tarball (${paths.size} files); launcher, bundles, types, README, ` +
      `license, and five platform optional dependencies present; no source/tests/fixtures; ` +
      "pnpm publish --dry-run succeeded",
  );
} finally {
  packed.cleanup();
}
