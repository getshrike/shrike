#!/usr/bin/env node
/**
 * Build the publishable `getshrike` CLI.
 *
 * Declarations come from `tsc`; the JavaScript is a single-file esbuild bundle
 * per public entry point so the published package does not depend on the
 * private `@shrike/*` workspace packages.
 */
import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync } from "node:fs";
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

// The TUI is a second, isolated bundle. It is not a runtime dependency and
// must not be inlined into `main.js`.
await build({
  absWorkingDir: cli,
  entryPoints: {
    tui: join(cli, "src", "tui", "main.tsx"),
  },
  outdir: dist,
  entryNames: "[name]",
  bundle: true,
  platform: "node",
  format: "esm",
  target: "node22",
  jsx: "automatic",
  splitting: false,
  sourcemap: false,
  legalComments: "none",
  logLevel: "warning",
  // Ink's dependencies still use `require()` for Node builtins. The ESM
  // bundle needs a real `require` for esbuild's dynamic-require shim, or the
  // first render throws "Dynamic require of \"assert\" is not supported".
  banner: {
    js: 'import { createRequire as __shrikeCreateRequire } from "node:module"; const require = __shrikeCreateRequire(import.meta.url);',
  },
  plugins: [{
    // Ink 7 statically bundles its DEV-only devtools module, which imports an
    // optional peer (`react-devtools-core`). Shrike never enables React
    // DevTools, so resolve it to an empty stub instead of adding a dependency.
    name: "stub-react-devtools",
    setup(build) {
      build.onResolve({ filter: /^react-devtools-core$/ }, () => ({
        path: "react-devtools-core",
        namespace: "stub",
      }));
      build.onLoad({ filter: /.*/, namespace: "stub" }, () => ({
        contents: "export default { initialize() {}, connectToDevTools() {} };",
        loader: "js",
      }));
    },
  }],
});

const externalWorkspaceImport = /(?:from\s*["']@shrike\/|import\(\s*["']@shrike\/|require\(\s*["']@shrike\/)/;
for (const name of ["main.js", "index.js", "config.js", "tui.js"]) {
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

// The authored skill lives at `skills/`; the installer loads it from `dist/`
// at runtime. Copy it after bundling so the published package has one copy.
const skills = join(root, "skills");
const distSkills = join(dist, "skills");
if (!existsSync(skills)) {
  console.error(`FAIL build-cli: ${skills} is missing`);
  process.exit(1);
}
rmSync(distSkills, { recursive: true, force: true });
cpSync(skills, distSkills, { recursive: true });
const requiredSkillFiles = [
  "shrike/SKILL.md",
  "shrike/references/commands.md",
  "shrike/references/workflows.md",
  "shrike/references/safety.md",
  "shrike/references/setup.md",
];
for (const path of requiredSkillFiles) {
  const full = join(distSkills, path);
  if (!existsSync(full) || readFileSync(full, "utf8").trim().length === 0) {
    console.error(`FAIL build-cli: dist/skills/${path} is missing or empty`);
    process.exit(1);
  }
}
const skillFiles = (function list(directory) {
  const entries = [];
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) entries.push(...list(path));
    else entries.push(path);
  }
  return entries;
})(distSkills);

console.log("OK: bundled getshrike CLI (dist/main.js, dist/index.js, dist/config.js, dist/tui.js)");
console.log(`OK: copied ${skillFiles.length} canonical skill files to dist/skills/`);
