#!/usr/bin/env node
/**
 * Differential check against the pinned reference tools.
 *
 * Shrike is deliberately more conservative than knip and depcheck. This script
 * makes the differences explicit instead of treating every disagreement as a
 * bug:
 *
 *   agreed       both Shrike and a reference tool report the symbol/package
 *   shrike-only  Shrike reports it and neither reference tool does
 *   tool-only    a reference tool reports it and Shrike does not
 *   internal-only  knip reports an export that Shrike classifies as used only
 *                  by its own file (not a deletion candidate)
 *
 * The fixture's `expected.json` pins the classification. A change in any
 * bucket fails CI, so a new false positive cannot hide behind "the tools
 * disagree".
 *
 * Usage:
 *   node scripts/differential.mjs [--fixture fixtures/differential] [--update]
 */
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

const root = resolve(new URL("..", import.meta.url).pathname);
const args = process.argv.slice(2);
const fixtureIndex = args.indexOf("--fixture");
const fixture = resolve(
  root,
  fixtureIndex >= 0 && args[fixtureIndex + 1] ? args[fixtureIndex + 1] : "fixtures/differential",
);
const update = args.includes("--update");
const engine = process.env.SHRIKE_ENGINE ?? join(root, "target", "debug", "shrike-engine");
const KNIP_VERSION = "5.88.1";
const DEPCHECK_VERSION = "1.4.7";
const npx = process.platform === "win32" ? "npx.cmd" : "npx";

function fail(message) {
  console.error(`FAIL: ${message}`);
  process.exit(1);
}

function run(command, commandArgs, options = {}) {
  return spawnSync(command, commandArgs, {
    cwd: fixture,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
    timeout: 300_000,
    ...options,
  });
}

function parseJson(text, label) {
  const start = text.indexOf("{");
  if (start < 0) fail(`${label} produced no JSON`);
  try {
    return JSON.parse(text.slice(start));
  } catch (error) {
    fail(`${label} produced invalid JSON: ${error.message}`);
  }
}

function sorted(values) {
  return [...values].sort();
}

function same(actual, expected) {
  return JSON.stringify(sorted(actual)) === JSON.stringify(sorted(expected));
}

if (!existsSync(join(fixture, "package.json"))) {
  fail(`no package.json in ${fixture}`);
}
if (!existsSync(engine)) {
  fail(`engine not built at ${engine}`);
}

// Shrike's own report.
const shrikeRun = run(process.execPath, [join(root, "packages", "cli", "src", "main.ts"), "scan", "--json"], {
  env: { ...process.env, SHRIKE_ENGINE: engine },
});
if (![0, 1].includes(shrikeRun.status)) {
  fail(`Shrike scan exited ${shrikeRun.status}: ${shrikeRun.stderr.trim()}`);
}
const shrike = parseJson(shrikeRun.stdout, "Shrike");
const shrikeCandidates = new Set((shrike.candidates ?? []).map((item) => item.symbol));
const shrikeInternal = new Set(shrike.snapshot?.internalOnly ?? []);
const shrikeUnused = new Set(
  (shrike.dependencies?.packages ?? []).flatMap((pkg) => pkg.unused.map((item) => item.name)),
);
const shrikeUnlisted = new Set(
  (shrike.dependencies?.packages ?? []).flatMap((pkg) => pkg.unlisted.map((item) => item.name)),
);

// knip: unused exports/types plus dependency issues.
const knipRun = run(npx, ["--yes", `knip@${KNIP_VERSION}`, "--reporter", "json", "--no-progress"]);
if (![0, 1].includes(knipRun.status)) {
  fail(`knip exited ${knipRun.status}: ${knipRun.stderr.trim()}`);
}
const knip = parseJson(knipRun.stdout, "knip");
const knipExports = new Set();
const knipUnused = new Set();
const knipUnlisted = new Set();
for (const issue of knip.issues ?? []) {
  for (const item of [...(issue.exports ?? []), ...(issue.types ?? [])]) {
    if (item?.name) knipExports.add(`${issue.file}#${item.name}`);
  }
  for (const name of [
    ...(issue.dependencies ?? []),
    ...(issue.devDependencies ?? []),
    ...(issue.optionalPeerDependencies ?? []),
  ]) {
    if (name?.name) knipUnused.add(name.name);
  }
  for (const name of issue.unlisted ?? []) {
    if (name?.name) knipUnlisted.add(name.name);
  }
}

// depcheck: unused and missing dependencies.
const depcheckRun = run(npx, ["--yes", `depcheck@${DEPCHECK_VERSION}`, "--json"]);
if (![0, 1, 255].includes(depcheckRun.status)) {
  fail(`depcheck exited ${depcheckRun.status}: ${depcheckRun.stderr.trim()}`);
}
const depcheck = parseJson(depcheckRun.stdout, "depcheck");
const depcheckUnused = new Set([
  ...(depcheck.dependencies ?? []),
  ...(depcheck.devDependencies ?? []),
]);
const depcheckMissing = new Set(Object.keys(depcheck.missing ?? {}));

const actual = {
  symbols: {
    agreed: [...shrikeCandidates].filter((symbol) => knipExports.has(symbol)),
    shrikeOnly: [...shrikeCandidates].filter((symbol) => !knipExports.has(symbol)),
    toolOnly: [...knipExports].filter(
      (symbol) => !shrikeCandidates.has(symbol) && !shrikeInternal.has(symbol),
    ),
    internalOnly: [...knipExports].filter((symbol) => shrikeInternal.has(symbol)),
  },
  dependencies: {
    agreedUnused: [...shrikeUnused].filter(
      (name) => knipUnused.has(name) || depcheckUnused.has(name),
    ),
    shrikeOnlyUnused: [...shrikeUnused].filter(
      (name) => !knipUnused.has(name) && !depcheckUnused.has(name),
    ),
    knipOnlyUnused: [...knipUnused].filter((name) => !shrikeUnused.has(name)),
    depcheckOnlyUnused: [...depcheckUnused].filter((name) => !shrikeUnused.has(name)),
    shrikeUnlisted: [...shrikeUnlisted],
    knipUnlisted: [...knipUnlisted],
    depcheckMissing: [...depcheckMissing],
  },
};
for (const group of Object.values(actual)) {
  for (const key of Object.keys(group)) group[key] = sorted(group[key]);
}

const expectedPath = join(fixture, "expected.json");
if (update) {
  writeFileSync(expectedPath, `${JSON.stringify(actual, null, 2)}\n`);
  console.log(`updated ${expectedPath}`);
} else {
  if (!existsSync(expectedPath)) fail(`missing ${expectedPath} (run with --update)`);
  const expected = JSON.parse(readFileSync(expectedPath, "utf8"));
  const mismatches = [];
  for (const [group, values] of Object.entries(actual)) {
    for (const [key, value] of Object.entries(values)) {
      const wanted = expected?.[group]?.[key] ?? [];
      if (!same(value, wanted)) {
        mismatches.push(`${group}.${key}: expected ${JSON.stringify(sorted(wanted))}, got ${JSON.stringify(value)}`);
      }
    }
  }
  if (mismatches.length > 0) {
    for (const mismatch of mismatches) console.error(`FAIL ${mismatch}`);
    process.exit(1);
  }
}

console.log(`Differential fixture: ${fixture}`);
console.log(
  `Shrike: ${shrikeCandidates.size} candidate(s), ${shrikeInternal.size} internal-only, ` +
    `${shrikeUnused.size} unused, ${shrikeUnlisted.size} unlisted`,
);
console.log(
  `knip ${KNIP_VERSION}: ${knipExports.size} unused export(s), ${knipUnused.size} unused dep(s)`,
);
console.log(`depcheck ${DEPCHECK_VERSION}: ${depcheckUnused.size} unused dep(s)`);
console.log(
  `symbols: agreed=${actual.symbols.agreed.length} shrike-only=${actual.symbols.shrikeOnly.length} ` +
    `tool-only=${actual.symbols.toolOnly.length} internal-only=${actual.symbols.internalOnly.length}`,
);
console.log(
  `dependencies: agreed=${actual.dependencies.agreedUnused.length} ` +
    `shrike-only=${actual.dependencies.shrikeOnlyUnused.length} ` +
    `knip-only=${actual.dependencies.knipOnlyUnused.length} ` +
    `depcheck-only=${actual.dependencies.depcheckOnlyUnused.length}`,
);
console.log("OK: differential classifications match expected.json");
