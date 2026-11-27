import test from "node:test";
import assert from "node:assert/strict";

import {
  computeMetric,
  intervalLength,
  isEligibleFile,
  mergeIntervals,
  removableLinesFor,
} from "../src/report/metrics.ts";
import { groupDirectory, normalizeReportPath } from "../src/report/groups.ts";

function file(path, overrides = {}) {
  return {
    path,
    contentHash: "blake3:0",
    moduleKind: "ts",
    parseStatus: "ok",
    generated: false,
    publicApi: false,
    lines: 10,
    ...overrides,
  };
}

function symbol(key, overrides = {}) {
  const [path, name] = key.split("#");
  return {
    key,
    name,
    kind: "function",
    file: path,
    span: { line: 1, column: 1 },
    exported: true,
    entry: false,
    publicApi: false,
    generated: false,
    typeOnly: false,
    lines: 1,
    ...overrides,
  };
}

function snapshot(files, symbols = []) {
  return { files, symbols };
}

test("merges overlapping and adjacent intervals before counting", () => {
  assert.deepEqual(
    mergeIntervals([
      { start: 8, end: 10 },
      { start: 1, end: 3 },
      { start: 3, end: 5 },
      { start: 4, end: 6 },
    ]),
    [
      { start: 1, end: 6 },
      { start: 8, end: 10 },
    ],
  );
  assert.equal(intervalLength([{ start: 1, end: 6 }, { start: 8, end: 10 }]), 9);
});

test("counts nested and overlapping spans once", () => {
  const files = [file("src/a.ts", { lines: 100 })];
  const symbols = [
    symbol("src/a.ts#outer", { span: { line: 10, column: 1 }, lines: 20 }),
    symbol("src/a.ts#inner", { span: { line: 15, column: 3 }, lines: 5 }),
    symbol("src/a.ts#adjacent", { span: { line: 30, column: 1 }, lines: 5 }),
    symbol("src/a.ts#touching", { span: { line: 35, column: 1 }, lines: 5 }),
  ];
  const keys = new Set(symbols.map((item) => item.key));

  const metric = computeMetric(snapshot(files, symbols), keys, new Set());

  // 10..29 (outer+inner) and 30..39 (adjacent+touching) = 30 lines.
  assert.equal(metric.removableLines, 30);
  assert.equal(metric.removableSymbols, 4);
  assert.equal(metric.removableFiles, 1);
  assert.equal(metric.analyzedLines, 100);
  assert.equal(metric.deadWeightBasisPoints, 3000);
  assert.equal(metric.uncountedRemovableSymbols, 0);
});

test("a whole file replaces its symbol intervals instead of adding to them", () => {
  const files = [file("src/dead.ts", { lines: 40 })];
  const symbols = [
    symbol("src/dead.ts#one", { span: { line: 1, column: 1 }, lines: 10 }),
    symbol("src/dead.ts#two", { span: { line: 20, column: 1 }, lines: 10 }),
  ];
  const keys = new Set(symbols.map((item) => item.key));

  const metric = computeMetric(snapshot(files, symbols), keys, new Set(["src/dead.ts"]));

  assert.equal(metric.removableLines, 40);
  assert.equal(metric.removableFiles, 1);
  assert.equal(metric.removableSymbols, 2);
  assert.equal(
    removableLinesFor(snapshot(files, symbols), keys, new Set(["src/dead.ts"])),
    40,
  );
});

test("excludes generated, unparsed, and unsupported files from both sides", () => {
  const files = [
    file("src/live.ts", { lines: 60 }),
    file("src/generated.ts", { lines: 100, generated: true }),
    file("src/broken.ts", { lines: 100, parseStatus: "error" }),
    file("src/content.mdx", { lines: 100, moduleKind: "mdx" }),
  ];
  const symbols = [
    symbol("src/generated.ts#g", { lines: 100 }),
    symbol("src/broken.ts#b", { lines: 100 }),
    symbol("src/content.mdx#m", { lines: 100 }),
    symbol("src/live.ts#dead", { lines: 5 }),
  ];
  const keys = new Set(symbols.map((item) => item.key));

  const metric = computeMetric(snapshot(files, symbols), keys, new Set());

  assert.equal(metric.analyzedFiles, 1);
  assert.equal(metric.analyzedLines, 60);
  assert.equal(metric.removableLines, 5);
  assert.equal(metric.removableSymbols, 4);
  assert.equal(metric.uncountedRemovableSymbols, 3);
  assert.ok(metric.removableLines <= metric.analyzedLines);
});

test("a whole-file path in an ineligible file is ignored", () => {
  const files = [file("src/generated.ts", { lines: 100, generated: true })];
  const metric = computeMetric(snapshot(files), new Set(), new Set(["src/generated.ts"]));

  assert.equal(metric.removableFiles, 0);
  assert.equal(metric.removableLines, 0);
  assert.equal(metric.deadWeightBasisPoints, 0);
});

test("handles the zero denominator and basis-point rounding", () => {
  const empty = computeMetric(snapshot([]), new Set(), new Set());
  assert.equal(empty.analyzedLines, 0);
  assert.equal(empty.removableLines, 0);
  assert.equal(empty.deadWeightBasisPoints, 0);

  const thirds = computeMetric(
    snapshot([file("src/a.ts", { lines: 3 })]),
    new Set(["src/a.ts#dead"]),
    new Set(),
  );
  // No symbol record means nothing is counted in the numerator.
  assert.equal(thirds.removableLines, 0);
  assert.equal(thirds.uncountedRemovableSymbols, 1);

  const one = computeMetric(
    snapshot([file("src/a.ts", { lines: 3 })], [symbol("src/a.ts#dead", { lines: 1 })]),
    new Set(["src/a.ts#dead"]),
    new Set(),
  );
  assert.equal(one.deadWeightBasisPoints, 3333);

  const all = computeMetric(
    snapshot([file("src/a.ts", { lines: 100 })], [symbol("src/a.ts#dead", { lines: 100 })]),
    new Set(["src/a.ts#dead"]),
    new Set(),
  );
  assert.equal(all.deadWeightBasisPoints, 10000);
});

test("counts module symbols only through their whole file", () => {
  const files = [file("src/dead.ts", { lines: 12 })];
  const symbols = [symbol("src/dead.ts#__module__", { kind: "module", lines: 1 })];
  const keys = new Set(["src/dead.ts#__module__"]);

  const metric = computeMetric(snapshot(files, symbols), keys, new Set(["src/dead.ts"]));

  assert.equal(metric.removableFiles, 1);
  assert.equal(metric.removableSymbols, 0);
  assert.equal(metric.removableLines, 12);
});

test("eligibility matches the documented module kinds", () => {
  for (const moduleKind of ["ts", "tsx", "js", "jsx", "mjs", "cjs", "dts"]) {
    assert.equal(isEligibleFile(file(`src/a.${moduleKind}`, { moduleKind })), true);
  }
  assert.equal(isEligibleFile(file("src/a.vue", { moduleKind: "vue" })), false);
  assert.equal(isEligibleFile(file("src/a.ts", { parseStatus: "error" })), false);
  assert.equal(isEligibleFile(file("src/a.ts", { generated: true })), false);
});

test("groups paths by the documented stable heuristic", () => {
  const cases = [
    ["src/features/billing/format.ts", "src/features/billing"],
    ["app/(marketing)/pricing/page.tsx", "app/(marketing)/pricing"],
    ["packages/ui/src/Button.tsx", "packages/ui"],
    ["apps/web/src/legacy.ts", "apps/web"],
    ["src/format.ts", "src"],
    ["index.ts", "."],
    ["src/modules/orders/line.ts", "src/modules/orders"],
  ];
  for (const [path, expected] of cases) {
    assert.equal(groupDirectory(path), expected, path);
  }
});

test("rejects absolute, backslash, and parent paths", () => {
  assert.throws(() => normalizeReportPath("/etc/passwd"));
  assert.throws(() => normalizeReportPath("src\\a.ts"));
  assert.throws(() => normalizeReportPath("src/../a.ts"));
  assert.equal(normalizeReportPath("src//a.ts"), "src/a.ts");
});
