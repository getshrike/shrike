import test from "node:test";
import assert from "node:assert/strict";

import {
  colorEnabled,
  renderReport,
  resolveWidth,
} from "../src/report/render.ts";

function summary(overrides = {}) {
  return {
    analyzedFiles: 3,
    analyzedLines: 100,
    removableFiles: 1,
    removableSymbols: 1,
    removableExports: 1,
    removableLines: 10,
    deadWeightBasisPoints: 1000,
    unusedDependencies: 0,
    protectedItems: 0,
    uncountedRemovableSymbols: 0,
    diagnostics: { total: 0, actionable: 0, informational: 0 },
    ...overrides,
  };
}

function currentReport(overrides = {}) {
  return {
    schemaVersion: "1.0",
    mode: "current",
    base: null,
    head: { commit: "9f2c1ab4d1c2b4f9a7e6d5c4b3a2918070605040", dirty: false },
    summary: summary(),
    groups: [
      {
        directory: "src",
        removableLines: 10,
        removableSymbols: 1,
        removableFiles: 1,
        items: [
          {
            id: "symbol:src/dead.ts#dead",
            kind: "symbol",
            classification: "never-used",
            path: "src/dead.ts",
            symbol: "src/dead.ts#dead",
            name: "dead",
            lines: 10,
            reason: "no-witness",
            confidence: "certain",
          },
        ],
      },
    ],
    protected: [],
    dependencies: [],
    diagnostics: { total: 0, actionable: 0, informational: 0, groups: [] },
    ...overrides,
  };
}

function branchReport(overrides = {}) {
  return currentReport({
    mode: "branch",
    base: { ref: "origin/main", commit: "1a2b3c4d5e6f708192a3b4c5d6e7f8091a2b3c4d" },
    groups: [
      {
        directory: "src/features/billing",
        removableLines: 10,
        removableSymbols: 1,
        removableFiles: 1,
        items: [
          {
            id: "symbol:src/features/billing/format.ts#formatInvoice",
            kind: "symbol",
            classification: "branch-orphan",
            path: "src/features/billing/format.ts",
            symbol: "src/features/billing/format.ts#formatInvoice",
            name: "formatInvoice",
            lines: 10,
            reason: "no-witness",
            confidence: "certain",
            trigger: {
              file: "src/features/billing/InvoiceCard.tsx",
              kind: "removed-import",
              change: "removed import",
              certainty: "certain",
            },
          },
        ],
      },
    ],
    ...overrides,
  });
}

function render(report, overrides = {}) {
  return renderReport(report, {
    width: 80,
    color: false,
    quiet: false,
    verbose: false,
    ...overrides,
  });
}

function assertWidth(text, width) {
  for (const line of text.split("\n")) {
    assert.ok(
      [...line].length <= width,
      `line exceeds ${width} columns (${[...line].length}): ${JSON.stringify(line)}`,
    );
  }
}

test("renders the current report at 80 columns", () => {
  const expected = [
    "Shrike report · current state · 9f2c1ab",
    "",
    "  Dead weight  10.0%",
    "  10 removable lines / 100 analyzed source lines",
    "  ████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░",
    "",
    "  1 file · 1 symbol",
    "",
    "  src  10 lines   1 symbol   1 file",
    "",
    "  Next: npx getshrike clean --all",
    "        npx getshrike report --verbose",
  ].join("\n");
  assert.equal(render(currentReport()), expected);
});

test("renders the branch report at 80 columns", () => {
  const expected = [
    "Shrike report · branch vs origin/main · 9f2c1ab",
    "",
    "  Dead weight  10.0%",
    "  10 removable lines / 100 analyzed source lines",
    "  ████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░",
    "",
    "  1 file · 1 symbol",
    "",
    "  src/features/billing  10 lines   1 symbol   1 file",
    "",
    "  Next: npx getshrike clean --base origin/main",
    "        npx getshrike why src/features/billing/format.ts#formatInvoice",
  ].join("\n");
  assert.equal(render(branchReport()), expected);
});

test("renders the empty report without celebration", () => {
  const report = currentReport({
    summary: summary({
      removableFiles: 0,
      removableSymbols: 0,
      removableExports: 0,
      removableLines: 0,
      deadWeightBasisPoints: 0,
    }),
    groups: [],
  });
  const text = render(report);
  assert.match(text, /0 removable lines \/ 100 analyzed source lines/);
  assert.match(text, /0 files · 0 symbols/);
  assert.match(text, /No removable dead weight found\./);
  assert.match(text, /Next: npx getshrike scan\n {8}npx getshrike install/);
  assert.doesNotMatch(text, /Great job|🎉|✨/);
});

test("renders n/a when there are no analyzable lines", () => {
  const report = currentReport({
    summary: summary({
      analyzedFiles: 0,
      analyzedLines: 0,
      removableFiles: 0,
      removableSymbols: 0,
      removableExports: 0,
      removableLines: 0,
      deadWeightBasisPoints: 0,
    }),
    groups: [],
  });
  const text = render(report);
  assert.match(text, /Dead weight {2}n\/a/);
  assert.match(text, /No analyzable source lines were found\./);
  assert.doesNotMatch(text, /[█░]/);
});

test("renders <0.1% when a positive numerator rounds to zero", () => {
  const report = currentReport({
    summary: summary({
      analyzedLines: 100000,
      removableLines: 1,
      deadWeightBasisPoints: 0,
    }),
  });
  const text = render(report);
  assert.match(text, /Dead weight {2}<0\.1%/);
  assert.match(text, /█░+/);
});

test("keeps the full layout at 120 columns", () => {
  const text = render(currentReport(), { width: 120 });
  assert.match(text, /Shrike report · current state · 9f2c1ab/);
  assert.equal(text.split("\n")[4].trim().length, 48);
  assert.match(text, /1 file · 1 symbol/);
  assertWidth(text, 120);
});

test("drops the bar and stacks counts at 40 columns", () => {
  const text = render(currentReport(), { width: 40 });
  assert.match(text, /Shrike report · current\n/);
  assert.match(text, /^  1 file$/m);
  assert.match(text, /^  1 symbol$/m);
  assert.match(text, /10 removable lines \/ 100 analyzed\n  source lines/);
  assertWidth(text, 40);
});

test("keeps every line within 40 and 60 columns for long paths", () => {
  const report = currentReport({
    groups: [
      {
        directory: "src/features/a-very-long-feature-name/nested",
        removableLines: 10,
        removableSymbols: 1,
        removableFiles: 1,
        items: [
          {
            id: "symbol:src/features/a-very-long-feature-name/nested/file.ts#dead",
            kind: "symbol",
            classification: "never-used",
            path: "src/features/a-very-long-feature-name/nested/file.ts",
            symbol: "src/features/a-very-long-feature-name/nested/file.ts#dead",
            name: "dead",
            lines: 10,
            reason: "no-witness",
            confidence: "certain",
          },
        ],
      },
    ],
  });
  for (const width of [40, 60, 80, 120]) {
    assertWidth(render(report, { width, verbose: true }), width);
  }
});

test("uses singular and plural counts correctly", () => {
  const one = render(currentReport());
  assert.match(one, /1 file · 1 symbol/);
  assert.doesNotMatch(one, /1 files|1 symbols/);

  const many = render(currentReport({
    summary: summary({
      removableFiles: 2,
      removableSymbols: 3,
      removableExports: 2,
      removableLines: 10,
    }),
    groups: [],
  }));
  assert.match(many, /2 files · 3 symbols \(2 exports\)/);
});

test("marks dependencies advisory and keeps them out of the numerator", () => {
  const report = currentReport({
    summary: summary({ unusedDependencies: 1 }),
    dependencies: [
      { package: "web", directory: "apps/web", name: "left-pad", kind: "unused", advisory: true },
    ],
  });
  const text = render(report, { verbose: true });
  assert.match(text, /1 dependency \(advisory\)/);
  assert.match(text, /Dependencies \(advisory\)/);
  assert.match(text, /web: left-pad {2}\[unused\]/);
  assert.match(text, /10 removable lines/);
});

test("shows protected reasons and diagnostics without verbose details", () => {
  const report = currentReport({
    summary: summary({
      removableSymbols: 0,
      removableExports: 0,
      removableFiles: 0,
      removableLines: 0,
      deadWeightBasisPoints: 0,
      protectedItems: 2,
      diagnostics: { total: 2, actionable: 1, informational: 1 },
    }),
    groups: [],
    protected: [
      { path: "packages/ui/src/Button.tsx", symbol: "packages/ui/src/Button.tsx#Button", reason: "public-api" },
      { path: "src/app/page.tsx", symbol: "src/app/page.tsx#Page", reason: "entry-role" },
    ],
    diagnostics: {
      total: 2,
      actionable: 1,
      informational: 1,
      groups: [
        { category: "parse", code: "parse_error", count: 1, actionable: true, files: ["src/broken.ts"] },
        { category: "asset-import", code: "unresolved_import", count: 1, actionable: false, files: ["src/a.tsx"] },
      ],
    },
  });
  const text = render(report);
  assert.match(text, /Protected reasons/);
  assert.match(text, /public-api {2}1/);
  assert.match(text, /entry-role {2}1/);
  assert.match(text, /Diagnostics {2}2 \(1 actionable\)/);
  assert.match(text, /parse: 1 · asset-import: 1/);
  assert.doesNotMatch(text, /e\.g\./);

  const verbose = render(report, { verbose: true });
  assert.match(verbose, /\(e\.g\. packages\/ui\/src\/Button\.tsx#Button\)/);
  assert.match(verbose, /parse: 1 × parse_error {2}\[src\/broken\.ts\]/);
});

test("quiet hides the sections but keeps the headline and next command", () => {
  const text = render(currentReport(), { quiet: true });
  assert.match(text, /Dead weight {2}10\.0%/);
  assert.match(text, /1 file · 1 symbol/);
  assert.match(text, /Next: npx getshrike clean --all/);
  assert.doesNotMatch(text, /src {2}10 lines/);
});

test("a dirty report warns and never prints a timestamp", () => {
  const report = currentReport({
    head: { commit: "9f2c1ab4d1c2b4f9a7e6d5c4b3a2918070605040", dirty: true },
  });
  const text = render(report);
  assert.match(text, /· dirty/);
  assert.match(text, /Dirty tree: this report is local feedback, not a CI receipt\./);
  assert.doesNotMatch(text, /\d{4}-\d{2}-\d{2}T|GMT|UTC|localhost|\/home\//);
});

test("next commands never use the bare short-name npx form", () => {
  const empty = currentReport({
    summary: summary({
      removableFiles: 0,
      removableSymbols: 0,
      removableExports: 0,
      removableLines: 0,
      deadWeightBasisPoints: 0,
    }),
    groups: [],
  });
  for (const report of [currentReport(), branchReport(), empty]) {
    assert.doesNotMatch(render(report), /npx shrike/);
  }
});

test("is byte-identical across repeated renders", () => {
  const report = currentReport();
  assert.equal(render(report), render(report));
  assert.equal(render(report, { verbose: true }), render(report, { verbose: true }));
});

test("resolves width from the flag, stream, env, then 80 and clamps to 40..120", () => {
  assert.equal(resolveWidth(72, 90, "100"), 72);
  assert.equal(resolveWidth(undefined, 90, "100"), 90);
  assert.equal(resolveWidth(undefined, undefined, "100"), 100);
  assert.equal(resolveWidth(undefined, undefined, undefined), 80);
  assert.equal(resolveWidth(20, undefined, undefined), 40);
  assert.equal(resolveWidth(400, undefined, undefined), 120);
  assert.equal(resolveWidth(undefined, undefined, "not-a-number"), 80);
});

test("enables colour only for a colour-capable TTY that did not opt out", () => {
  assert.equal(colorEnabled(true, {}, false), true);
  assert.equal(colorEnabled(false, {}, false), false);
  assert.equal(colorEnabled(true, { NO_COLOR: "1" }, false), false);
  assert.equal(colorEnabled(true, { NO_COLOR: "" }, false), false);
  assert.equal(colorEnabled(true, { FORCE_COLOR: "0" }, false), false);
  assert.equal(colorEnabled(true, { TERM: "dumb" }, false), false);
  assert.equal(colorEnabled(true, {}, true), false);
});

test("emits no ANSI bytes when colour is disabled", () => {
  const text = render(currentReport(), {
    color: colorEnabled(true, { NO_COLOR: "1" }, false),
  });
  assert.equal(text.includes("\x1b["), false);
});
