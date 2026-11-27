import test from "node:test";
import assert from "node:assert/strict";
import { createElement } from "react";
import { render } from "ink-testing-library";

import { ReportApp } from "../src/tui/view.ts";

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

function report(overrides = {}) {
  return {
    schemaVersion: "1.0",
    mode: "current",
    base: null,
    head: { commit: "9f2c1ab4d1c2b4f9a7e6d5c4b3a2918070605040", dirty: false },
    summary: {
      analyzedFiles: 3,
      analyzedLines: 100,
      removableFiles: 1,
      removableSymbols: 2,
      removableExports: 2,
      removableLines: 20,
      deadWeightBasisPoints: 2000,
      unusedDependencies: 1,
      protectedItems: 1,
      uncountedRemovableSymbols: 0,
      diagnostics: { total: 1, actionable: 1, informational: 0 },
    },
    groups: [
      {
        directory: "src/features/billing",
        removableLines: 20,
        removableSymbols: 2,
        removableFiles: 0,
        items: [item("src/features/billing/format.ts")],
      },
    ],
    protected: [{ path: "packages/ui/src/Button.tsx", symbol: "packages/ui/src/Button.tsx#Button", reason: "public-api" }],
    dependencies: [{ package: "web", directory: "apps/web", name: "left-pad", kind: "unused", advisory: true }],
    diagnostics: {
      total: 1,
      actionable: 1,
      informational: 0,
      groups: [{ category: "parse", code: "parse_error", count: 1, actionable: true, files: ["src/broken.ts"] }],
    },
    ...overrides,
  };
}

function mount(value = report(), props = {}) {
  const instance = render(createElement(ReportApp, { report: value, width: 80, height: 24, ...props }));
  return instance;
}

function frame(instance) {
  return instance.lastFrame() ?? "";
}

async function press(instance, input) {
  instance.stdin.write(input);
  await new Promise((resolve) => setTimeout(resolve, 30));
}

test("renders the header, dead weight, counts, groups, and next command", () => {
  const instance = mount();
  const text = frame(instance);
  assert.match(text, /Shrike · current state · 9f2c1ab/);
  assert.match(text, /Dead weight {2}20\.0%/);
  assert.match(text, /20 removable lines \/ 100 analyzed source lines/);
  assert.match(text, /1 file · 2 symbols · 1 dependency \(advisory\)/);
  assert.match(text, /1 protected · 1 diagnostic \(1 actionable\)/);
  assert.match(text, /src\/features\/billing/);
  assert.match(text, /packages\/ui\/src\/Button\.tsx/);
  assert.match(text, /q quit/);
  instance.unmount();
});

test("renders the empty state without a broken bar", () => {
  const empty = report({
    summary: {
      ...report().summary,
      removableFiles: 0,
      removableSymbols: 0,
      removableExports: 0,
      removableLines: 0,
      deadWeightBasisPoints: 0,
      analyzedLines: 0,
    },
    groups: [],
    protected: [],
    dependencies: [],
    diagnostics: { total: 0, actionable: 0, informational: 0, groups: [] },
  });
  const instance = mount(empty);
  const text = frame(instance);
  assert.match(text, /No removable dead weight found\./);
  assert.match(text, /Next: npx getshrike scan/);
  assert.doesNotMatch(text, /[█░]/);
  instance.unmount();
});

test("handles a narrow terminal without exceeding the width", () => {
  const instance = mount(report(), { width: 40, height: 16 });
  const text = frame(instance);
  for (const line of text.split("\n")) {
    assert.ok([...line].length <= 40, `line too wide: ${JSON.stringify(line)}`);
  }
  instance.unmount();
});

test("question mark toggles help and q exits", async () => {
  const instance = mount();
  await press(instance, "?");
  assert.match(frame(instance), /Keys/);
  assert.match(frame(instance), /read-only/);
  await press(instance, "?");
  assert.doesNotMatch(frame(instance), /This view is read-only/);
  await press(instance, "q");
  instance.unmount();
});

test("enter opens group detail and esc returns", async () => {
  const instance = mount();
  await press(instance, "\r");
  const text = frame(instance);
  assert.match(text, /Details/);
  assert.match(text, /src\/features\/billing/);
  assert.match(text, /20 removable lines · 2 symbols · 0 files/);
  assert.match(text, /src\/features\/billing\/format\.ts#dead/);
  await press(instance, "\u001b");
  assert.doesNotMatch(frame(instance), /Details/);
  instance.unmount();
});

test("down arrow selects an item and enter shows its exact next command", async () => {
  const instance = mount();
  await press(instance, "\u001b[B");
  await press(instance, "\r");
  const text = frame(instance);
  assert.match(text, /Details/);
  assert.match(text, /10 lines · certain · never-used/);
  assert.match(text, /No witness from any entry reaches this symbol\./);
  assert.match(text, /Next: npx getshrike why src\/features\/billing\/format\.ts#dead/);
  await press(instance, "\u001b");
  assert.doesNotMatch(frame(instance), /Next: npx getshrike why/);
  instance.unmount();
});

test("c shows the copyable next command and d/p toggle summaries", async () => {
  const instance = mount();
  await press(instance, "c");
  assert.match(frame(instance), /Next command/);
  assert.match(frame(instance), /npx getshrike clean --all/);
  await press(instance, "c");
  await press(instance, "d");
  assert.match(frame(instance), /parse: 1 × parse_error/);
  await press(instance, "d");
  await press(instance, "p");
  assert.match(frame(instance), /public-api {2}1/);
  instance.unmount();
});

test("filters rows by a case-sensitive path substring", async () => {
  const instance = mount();
  await press(instance, "/");
  await press(instance, "Button");
  const text = frame(instance);
  assert.match(text, /packages\/ui\/src\/Button\.tsx/);
  assert.doesNotMatch(text, /src\/features\/billing {2}20 lines/);
  assert.match(text, /Filter: Button/);
  instance.unmount();
});

test("shows whole-file detail without offering why", async () => {
  const fileReport = report({
    groups: [{
      directory: "src/legacy",
      removableLines: 42,
      removableSymbols: 0,
      removableFiles: 1,
      items: [{
        id: "file:src/legacy/OldDashboard.tsx",
        kind: "file",
        classification: "never-used",
        path: "src/legacy/OldDashboard.tsx",
        lines: 42,
        reason: "no-witness",
        confidence: "certain",
      }],
    }],
  });
  const instance = mount(fileReport);
  await press(instance, "\u001b[B");
  await press(instance, "\r");
  const text = frame(instance);
  assert.match(text, /Details/);
  assert.match(text, /src\/legacy\/OldDashboard\.tsx/);
  assert.match(text, /No entry and no incoming import reaches this file\./);
  assert.doesNotMatch(text, /npx getshrike why/);
  assert.match(text, /Next: npx getshrike clean --all/);
  instance.unmount();
});

test("shows protected detail and states it will not be deleted", async () => {
  const instance = mount();
  await press(instance, "/");
  await press(instance, "Button");
  await press(instance, "\r");
  await press(instance, "\r");
  const text = frame(instance);
  assert.match(text, /Details/);
  assert.match(text, /protected: public-api/);
  assert.match(text, /This item will not be deleted\./);
  assert.doesNotMatch(text, /Delete/);
  instance.unmount();
});

test("shows dependency detail as advisory with no automatic removal", async () => {
  const instance = mount();
  await press(instance, "/");
  await press(instance, "left-pad");
  await press(instance, "\r");
  await press(instance, "\r");
  const text = frame(instance);
  assert.match(text, /Details/);
  assert.match(text, /web: left-pad/);
  assert.match(text, /unused dependency · advisory/);
  assert.match(text, /Shrike will not remove this automatically\./);
  instance.unmount();
});

test("there is no delete action", () => {
  const instance = mount();
  assert.doesNotMatch(frame(instance), /\bDelete\b|\bRemove\b/);
  instance.unmount();
});
