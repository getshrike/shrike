#!/usr/bin/env node
/**
 * Stage a native engine into its platform package.
 *
 * The release workflow runs this after `cargo build --release` for each target.
 * The staged package contains the binary, its SHA-256 sidecar, a short README,
 * and the repository license.
 */
import { createHash } from "node:crypto";
import { chmodSync, copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const PLATFORMS = {
  "linux-x64": { os: "linux", cpu: "x64", binary: "shrike-engine" },
  "linux-arm64": { os: "linux", cpu: "arm64", binary: "shrike-engine" },
  "darwin-x64": { os: "darwin", cpu: "x64", binary: "shrike-engine" },
  "darwin-arm64": { os: "darwin", cpu: "arm64", binary: "shrike-engine" },
  "win32-x64": { os: "win32", cpu: "x64", binary: "shrike-engine.exe" },
};
// npm's spam filter rejects the `getshrike-win32-x64` name. Keep the platform
// key for Node's `win32` naming and publish the package as `windows`.
const PACKAGE_NAMES = { "win32-x64": "getshrike-windows-x64" };

const { values } = parseArgs({
  options: {
    platform: { type: "string" },
    binary: { type: "string" },
    version: { type: "string" },
  },
});

const platform = values.platform;
const spec = platform ? PLATFORMS[platform] : undefined;
if (!platform || !spec) {
  console.error(`FAIL stage-platform: --platform must be one of ${Object.keys(PLATFORMS).join(", ")}`);
  process.exit(1);
}
if (!values.binary) {
  console.error("FAIL stage-platform: --binary is required");
  process.exit(1);
}

const source = resolve(values.binary);
if (!existsSync(source)) {
  console.error(`FAIL stage-platform: binary not found: ${source}`);
  process.exit(1);
}

const packageDir = join(root, "packages", "platform", platform);
const manifestPath = join(packageDir, "package.json");
if (!existsSync(manifestPath)) {
  console.error(`FAIL stage-platform: package manifest not found: ${manifestPath}`);
  process.exit(1);
}
const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
const expectedName = PACKAGE_NAMES[platform] ?? `getshrike-${platform}`;
if (manifest.name !== expectedName) {
  console.error(`FAIL stage-platform: expected ${expectedName}, found ${manifest.name}`);
  process.exit(1);
}
if (manifest.os?.[0] !== spec.os || manifest.cpu?.[0] !== spec.cpu) {
  console.error(`FAIL stage-platform: ${platform} manifest os/cpu does not match ${spec.os}/${spec.cpu}`);
  process.exit(1);
}
if (values.version) {
  manifest.version = values.version;
  writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
}

const binDir = join(packageDir, "bin");
mkdirSync(binDir, { recursive: true });
const target = join(binDir, spec.binary);
copyFileSync(source, target);
if (process.platform !== "win32") chmodSync(target, 0o755);

const digest = createHash("sha256").update(readFileSync(target)).digest("hex");
writeFileSync(`${target}.sha256`, `${digest}  ${spec.binary}\n`);

writeFileSync(
  join(packageDir, "README.md"),
  `# ${manifest.name}\n\n` +
    `Native Shrike engine for ${spec.os} ${spec.cpu}. This package is installed\n` +
    "automatically as an optional dependency of\n" +
    "[`getshrike`](https://www.npmjs.com/package/getshrike). Do not install it\n" +
    "directly.\n\n" +
    `The \`shrike\` launcher verifies \`bin/${spec.binary}\` against\n` +
    `\`bin/${spec.binary}.sha256\` before running it.\n`,
);

copyFileSync(join(root, "LICENSE"), join(packageDir, "LICENSE"));

console.log(
  `OK: staged ${manifest.name}@${manifest.version} ` +
    `(${basename(source)} -> bin/${spec.binary}, sha256 ${digest.slice(0, 12)}...)`,
);
