#!/usr/bin/env node
/**
 * Build the publishable `getshrike` CLI.
 *
 * Declarations come from `tsc`; the JavaScript is a single-file esbuild bundle
 * per public entry point so the published package does not depend on the
 * private `@shrike/*` workspace packages.
 */
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, rmSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { build } from "esbuild";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const cli = join(root, "packages", "cli");
const dist = join(cli, "dist");
const tsc = join(root, "node_modules", "typescript", "bin", "tsc");

if (!existsSync(tsc)) {
  console.error("FAIL build-cli: typescript is not installed; run `pnpm install`");
  process.exit(1);
}

rmSync(dist, { recursive: true, force: true });
mkdirSync(dist, { recursive: true });

const declarations = spawnSync(process.execPath, [tsc, "-p", join(cli, "tsconfig.build.json")], {
  cwd: root,
  stdio: "inherit",
});
if (declarations.status !== 0) process.exit(declarations.status ?? 1);

await build({
  absWorkingDir: cli,
  entryPoints: {
    main: join(cli, "src", "main.ts"),
    index: join(cli, "src", "index.ts"),
    config: join(cli, "src", "config.ts"),
  },
  outdir: dist,
  entryNames: "[name]",
  bundle: true,
  platform: "node",
  format: "esm",
  target: "node22",
  splitting: false,
  sourcemap: false,
  legalComments: "none",
  logLevel: "warning",
});

const externalWorkspaceImport = /(?:from\s*["']@shrike\/|import\(\s*["']@shrike\/|require\(\s*["']@shrike\/)/;
for (const name of ["main.js", "index.js", "config.js"]) {
  const path = join(dist, name);
  if (!existsSync(path)) {
    console.error(`FAIL build-cli: ${path} was not produced`);
    process.exit(1);
  }
  const text = readFileSync(path, "utf8");
  if (externalWorkspaceImport.test(text)) {
    console.error(`FAIL build-cli: ${name} still imports a private @shrike package`);
    process.exit(1);
  }
}

console.log("OK: bundled getshrike CLI (dist/main.js, dist/index.js, dist/config.js)");
