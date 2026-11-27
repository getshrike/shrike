import test from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync, statSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

/**
 * The published CLI keeps Ink and React out of `dist/main.js`.
 *
 * These assertions read the built artifacts, not source. They run only after
 * `pnpm run build:cli`; a plain unit-test checkout skips them instead of
 * failing on a missing dist directory.
 */
const root = resolve(fileURLToPath(new URL("../../..", import.meta.url)));
const dist = join(root, "packages", "cli", "dist");
const mainPath = join(dist, "main.js");
const tuiPath = join(dist, "tui.js");
const built = existsSync(mainPath) && existsSync(tuiPath);

const TUI_MARKERS = [
  'Symbol.for("react.element")',
  'Symbol.for("react.transitional.element")',
  "ink-root",
  "ink-box",
  "__REACT_DEVTOOLS_GLOBAL_HOOK__",
];

test("dist/main.js never contains Ink, React, or Yoga", { skip: !built }, () => {
  const main = readFileSync(mainPath, "utf8");
  for (const marker of TUI_MARKERS) {
    assert.ok(!main.includes(marker), `main bundle contains ${marker}`);
  }
  assert.ok(!/\byoga\b/i.test(main), "main bundle contains Yoga");
  assert.ok(!/(?:from|import\()\s*["'](?:ink|react)["']/.test(main), "main bundle statically imports Ink or React");
});

test("dist/tui.js is a separate bundle with the Ink entry and a 4 MB ceiling", { skip: !built }, () => {
  const tui = readFileSync(tuiPath, "utf8");
  assert.ok(tui.includes('Symbol.for("react.element")'), "TUI bundle does not contain React");
  assert.ok(tui.includes("ink-root"), "TUI bundle does not contain Ink");
  assert.ok(statSync(tuiPath).size <= 4 * 1024 * 1024, "TUI bundle exceeds the 4 MB ceiling");
  assert.ok(!tui.includes('from "@shrike/'), "TUI bundle imports a private workspace package");
});

test("dist/tui.js can load Node builtins at render time", { skip: !built }, async () => {
  // The bundle is ESM, but Ink's dependency graph still calls `require()` for
  // builtins such as `assert`. Without a real `require`, the first render
  // throws and the CLI silently falls back to the plain report.
  const tui = readFileSync(tuiPath, "utf8");
  assert.match(
    tui,
    /import \{ createRequire as __shrikeCreateRequire \} from "node:module"/,
    "TUI bundle has no ESM require shim",
  );
  assert.match(
    tui,
    /const require = __shrikeCreateRequire\(import\.meta\.url\)/,
    "TUI bundle does not bind require to the bundle URL",
  );
  const imported = await import(pathToFileURL(tuiPath).href);
  assert.equal(typeof imported.runTui, "function", "TUI bundle does not export runTui");
});

test("the loader uses a non-literal specifier so esbuild cannot inline the TUI", () => {
  const loader = readFileSync(join(root, "packages", "cli", "src", "tui", "load.ts"), "utf8");
  assert.match(loader, /const specifier = "\.\/tui\.js";/);
  assert.match(loader, /await import\(specifier\)/);
});

test("the build emits both bundles and keeps tui.js out of main.js", { skip: !built }, () => {
  const main = readFileSync(mainPath, "utf8");
  assert.ok(!main.includes("dist/tui.js"), "main bundle embeds the TUI path");
  assert.ok(main.includes("getshrike-linux-x64"), "main bundle lost the platform resolver");
  assert.ok(statSync(mainPath).size <= 1024 * 1024, "main bundle exceeds the 1 MB sanity ceiling");
});
