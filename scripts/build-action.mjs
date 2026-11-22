#!/usr/bin/env node
/** Build the committed GitHub Action entry point. */
import { existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { build } from "esbuild";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const action = join(root, "packages", "action");
const outfile = join(action, "dist", "main.cjs");

await build({
  absWorkingDir: action,
  entryPoints: [join(action, "src", "main.ts")],
  outfile,
  bundle: true,
  platform: "node",
  format: "cjs",
  target: "node24",
  sourcemap: false,
  legalComments: "none",
  logLevel: "warning",
});

if (!existsSync(outfile)) {
  console.error("FAIL build-action: dist/main.cjs was not produced");
  process.exit(1);
}

console.log("OK: bundled GitHub Action (packages/action/dist/main.cjs)");
