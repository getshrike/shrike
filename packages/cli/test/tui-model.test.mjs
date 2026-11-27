import test from "node:test";
import assert from "node:assert/strict";

import {
  buildTuiModel,
  clampSelection,
  detailLines,
  filterRows,
  itemRows,
  rowPath,
  selectRows,
} from "../src/tui/model.ts";

function item(path, overrides = {}) {
  return {
    id: `symbol:${path}#dead`,
    kind: "symbol",
    classification: "never-used",
    path,
    symbol: `${path}#dead`,
    name: "dead",
    lines: 10,
    reason: "no-witness",
    confidence: "certain",
    ...overrides,
  };
}

function group(directory, lines, overrides = {}) {
  return {
    directory,
    removableLines: lines,
    removableSymbols: 1,
    removableFiles: 0,
    items: [item(`${directory}/dead.ts`)],
    ...overrides,
  };
}

function report(overrides = {}) {
  return {
    schemaVersion: "1.0",
    mode: "current",
    base: null,
    head: { commit: "9f2c1ab4d1c2b4f9a7e6d5c4b3a2918070605040", dirty: false },
    summary: {
      analyzedFiles: 3,
      analyzedLines: 100,
      removableFiles: 0,
      removableSymbols: 2,
      removableExports: 2,
      removableLines: 20,
      deadWeightBasisPoints: 2000,
      unusedDependencies: 1,
      protectedItems: 1,
      uncountedRemovableSymbols: 0,
      diagnostics: { total: 0, actionable: 0, informational: 0 },
    },
    groups: [group("src/b", 5), group("src/a", 15)],
    protected: [{ path: "packages/ui/src/Button.tsx", symbol: "packages/ui/src/Button.tsx#Button", reason: "public-api" }],
    dependencies: [{ package: "web", directory: "apps/web", name: "left-pad", kind: "unused", advisory: true }],
    diagnostics: { total: 0, actionable: 0, informational: 0, groups: [] },
    ...overrides,
  };
}

test("builds group rows followed by their items in the plain renderer's order", () => {
  const model = buildTuiModel(report());
  assert.deepEqual(model.groups.map((entry) => entry.directory), ["src/a", "src/b"]);
  assert.deepEqual(model.rows.map(rowPath), [
    "src/a",
    "src/a/dead.ts",
    "src/b",
    "src/b/dead.ts",
    "packages/ui/src/Button.tsx",
    "apps/web/left-pad",
  ]);
  assert.deepEqual(
    model.rows.map((row) => row.kind),
    ["group", "item", "group", "item", "protected", "dependency"],
  );
  assert.deepEqual(model.next, ["npx getshrike clean --all", "npx getshrike report --verbose"]);
});

test("filters by path, is case-sensitive, and clamps selection", () => {
  const model = buildTuiModel(report());
  assert.deepEqual(filterRows(model.rows, "src/").map(rowPath), [
    "src/a",
    "src/a/dead.ts",
    "src/b",
    "src/b/dead.ts",
    "packages/ui/src/Button.tsx",
  ]);
  assert.deepEqual(filterRows(model.rows, "SRC/"), []);
  assert.deepEqual(selectRows(model, "Button", 50), {
    rows: [model.rows[4]],
    selection: 0,
  });
  assert.equal(clampSelection([], 9), 0);
  assert.equal(clampSelection(model.rows, -2), 0);
  assert.equal(clampSelection(model.rows, 99), model.rows.length - 1);
});

test("group rows keep their items for group detail", () => {
  const model = buildTuiModel(report());
  const first = model.rows[0];
  assert.equal(first.kind, "group");
  const items = itemRows(first);
  assert.equal(items.length, 1);
  assert.equal(items[0].kind, "item");
  assert.equal(model.rows[1].kind, "item");
  assert.match(detailLines(model, items[0]).join("\n"), /No witness from any entry/);
});

test("builds a large view model without quadratic work", () => {
  const groups = [];
  for (let index = 0; index < 10_000; index += 1) {
    groups.push(group(`src/g${String(index).padStart(5, "0")}`, index % 100));
  }
  const started = performance.now();
  const model = buildTuiModel(report({ groups }));
  const elapsed = performance.now() - started;
  assert.equal(model.rows.filter((row) => row.kind === "group").length, 10_000);
  assert.ok(elapsed < 2_000, `view model took ${elapsed.toFixed(0)}ms`);
});

test("does not mutate the report", () => {
  const input = report();
  const before = JSON.stringify(input);
  const model = buildTuiModel(input);
  filterRows(model.rows, "src");
  detailLines(model, model.rows[0]);
  assert.equal(JSON.stringify(input), before);
});
