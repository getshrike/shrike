import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import Ajv2020 from "ajv/dist/2020.js";

import {
  buildBranchReport,
  buildCurrentReport,
  dependencyItems,
  firstWhySymbol,
  normalizeProtectedReason,
  reportDiagnostics,
  reportExitCode,
} from "../src/report/build.ts";
import { summarizeDiagnostics } from "../src/diagnostics.ts";

const schemaPath = fileURLToPath(
  new URL("../../../schemas/cleanup-report.schema.json", import.meta.url),
);
const schema = JSON.parse(readFileSync(schemaPath, "utf8"));
const validateReport = new Ajv2020({ allErrors: true, strict: false }).compile(schema);

function file(path, overrides = {}) {
  return {
    path,
    contentHash: "blake3:0",
    moduleKind: "ts",
    parseStatus: "ok",
    generated: false,
    publicApi: false,
    package: "fixture",
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

function snapshot(files, symbols = [], overrides = {}) {
  return {
    files,
    symbols,
    candidates: [],
    unusedFiles: [],
    entries: [],
    proofs: [],
    unknowns: [],
    internalOnly: [],
    diagnostics: [],
    graphHash: "blake3:fixture",
    ...overrides,
  };
}

function dependencies(packages = []) {
  const summary = {
    packages: packages.length,
    unused: 0,
    unlisted: 0,
    transitive: 0,
    unusedByKind: {
      dependencies: 0,
      devDependencies: 0,
      optionalDependencies: 0,
      peerDependencies: 0,
    },
  };
  return { packages, summary };
}

function currentInput(snap, overrides = {}) {
  return {
    snapshot: snap,
    headCommit: "a".repeat(40),
    dirty: false,
    dependencies: dependencies(),
    diagnostics: summarizeDiagnostics(snap.diagnostics ?? []),
    ...overrides,
  };
}

function branchInput(head, delta, plan, overrides = {}) {
  return {
    head,
    delta,
    plan,
    baseRef: "origin/main",
    baseCommit: "b".repeat(40),
    headCommit: "a".repeat(40),
    dirty: false,
    dependencies: dependencies(),
    diagnostics: summarizeDiagnostics(head.diagnostics ?? []),
    ...overrides,
  };
}

function emptyPlan(overrides = {}) {
  return {
    schemaVersion: "1.0",
    id: "plan_12345678",
    base: { ref: "origin/main", commit: "b".repeat(40) },
    head: { commit: "a".repeat(40), dirty: false },
    deltaHash: "blake3:0",
    totals: { symbols: 0, files: 0, lines: 0 },
    batches: [],
    blocked: [],
    policy: {
      entryPolicy: "local",
      maxSymbols: 50,
      maxFiles: 10,
      allowOversized: false,
    },
    diagnostics: [],
    ...overrides,
  };
}

test("builds a schema-valid current report from snapshot facts only", () => {
  const files = [
    file("src/lib/keep.ts", { lines: 20 }),
    file("src/features/billing/legacy.ts", { lines: 100 }),
    file("src/features/billing/format.ts", { lines: 50 }),
    file("src/generated.ts", { lines: 999, generated: true }),
  ];
  const symbols = [
    symbol("src/lib/keep.ts#keep", { lines: 20 }),
    symbol("src/features/billing/format.ts#formatInvoice", { lines: 12 }),
    symbol("src/features/billing/format.ts#kept", { lines: 8 }),
    symbol("src/generated.ts#generated", { lines: 1 }),
    symbol("src/unknown.ts#orphan", { lines: 4 }),
  ];
  const snap = snapshot(files, symbols, {
    candidates: [
      "src/features/billing/format.ts#formatInvoice",
      "src/generated.ts#generated",
      "src/unknown.ts#orphan",
    ],
    unusedFiles: ["src/features/billing/legacy.ts"],
    proofs: [
      { symbol: "src/features/billing/format.ts#formatInvoice", reason: "no witness; isolated symbol", incoming: [], outgoing: [] },
      { symbol: "src/features/billing/format.ts#kept", reason: "blocked by public API", incoming: [], outgoing: [] },
    ],
    unknowns: [
      { reason: "dynamic import", scope: "module", file: "src/lib/keep.ts", span: { line: 1, column: 1 } },
    ],
    internalOnly: ["src/lib/keep.ts#keep"],
    diagnostics: [
      { code: "parse_error", severity: "error", message: "cannot parse", file: "src/broken.ts" },
      { code: "asset_import", severity: "info", message: "`./x.css` is not analyzed as source", file: "src/a.tsx" },
    ],
  });

  const report = buildCurrentReport(currentInput(snap));

  assert.equal(validateReport(report), true, JSON.stringify(validateReport.errors));
  assert.equal(report.mode, "current");
  assert.equal(report.base, null);
  assert.equal(report.summary.analyzedFiles, 3);
  assert.equal(report.summary.analyzedLines, 170);
  assert.equal(report.summary.removableFiles, 2);
  assert.equal(report.summary.removableSymbols, 3);
  assert.equal(report.summary.removableExports, 3);
  assert.equal(report.summary.removableLines, 112);
  assert.equal(report.summary.deadWeightBasisPoints, 6588);
  assert.equal(report.summary.uncountedRemovableSymbols, 2);
  assert.equal(report.summary.diagnostics.total, 2);
  assert.equal(report.summary.diagnostics.actionable, 1);

  assert.deepEqual(report.groups.map((group) => group.directory), [
    "src/features/billing",
  ]);
  const billing = report.groups[0];
  assert.equal(billing.removableLines, 112);
  assert.equal(billing.removableFiles, 2);
  assert.deepEqual(
    billing.items.map((item) => item.id),
    [
      "symbol:src/features/billing/format.ts#formatInvoice",
      "file:src/features/billing/legacy.ts",
    ],
  );
  assert.equal(billing.items[0].classification, "never-used");
  assert.equal(billing.items[0].reason, "no-witness");
  assert.equal(billing.items[1].reason, "whole-file");

  assert.deepEqual(
    report.protected.map((item) => [item.reason, item.path]),
    [
      ["public-api", "src/features/billing/format.ts"],
      ["unknown-edge", "src/lib/keep.ts"],
      ["other", "src/lib/keep.ts"],
    ],
  );
  assert.equal(report.protected[0].detail, "blocked by public API");
  assert.equal(reportExitCode(report), 1);
});

test("builds a schema-valid branch report with triggers and pre-existing counts", () => {
  const files = [
    file("src/keep.ts", { lines: 10 }),
    file("src/lib/preexisting.ts", { lines: 30 }),
    file("src/lib/format.ts", { lines: 100 }),
    file("src/features/billing/totals.ts", { lines: 50 }),
    file("src/features/billing/format.ts", { lines: 40 }),
  ];
  const symbols = [
    symbol("src/keep.ts#keep", { lines: 10 }),
    symbol("src/lib/preexisting.ts#old", { lines: 5 }),
    symbol("src/lib/format.ts#__module__", { kind: "module", lines: 1 }),
    symbol("src/lib/format.ts#helper", { lines: 8 }),
    symbol("src/features/billing/totals.ts#totals", { lines: 20 }),
    symbol("src/features/billing/format.ts#formatInvoice", { lines: 12 }),
  ];
  const head = snapshot(files, symbols, {
    candidates: [
      "src/lib/preexisting.ts#old",
      "src/features/billing/format.ts#formatInvoice",
      "src/features/billing/totals.ts#totals",
    ],
    unusedFiles: ["src/lib/format.ts"],
  });
  const delta = {
    schemaVersion: "1.0",
    base: { ref: "origin/main", commit: "b".repeat(40) },
    head: { commit: "a".repeat(40), dirty: false },
    newlyDead: [
      {
        symbol: "src/features/billing/format.ts#formatInvoice",
        kind: "function",
        file: "src/features/billing/format.ts",
        lastWitness: ["src/features/billing/InvoiceCard.tsx#__module__", "src/features/billing/format.ts#formatInvoice"],
        trigger: {
          file: "src/features/billing/InvoiceCard.tsx",
          kind: "removed-import",
          change: "removed import of formatInvoice",
          certainty: "certain",
        },
        confidence: "certain",
        estimatedLines: 12,
      },
    ],
    newlyDeadFiles: ["src/lib/format.ts"],
    cascade: { symbols: 1, files: 1, lines: 20 },
    batches: [],
    incomparable: null,
    diagnostics: [],
  };
  const plan = emptyPlan({
    totals: { symbols: 3, files: 3, lines: 33 },
    batches: [
      {
        id: "batch_1",
        symbols: [
          "src/features/billing/format.ts#formatInvoice",
          "src/features/billing/totals.ts#totals",
          "src/lib/format.ts#__module__",
        ],
        files: [
          "src/features/billing/format.ts",
          "src/features/billing/totals.ts",
          "src/lib/format.ts",
        ],
        lines: 33,
      },
    ],
    blocked: [
      {
        symbol: "src/keep.ts#keep",
        reason: "blocked by public API",
        incoming: 1,
        outgoing: 0,
      },
    ],
  });

  const report = buildBranchReport(branchInput(head, delta, plan));

  assert.equal(validateReport(report), true, JSON.stringify(validateReport.errors));
  assert.equal(report.mode, "branch");
  assert.deepEqual(report.base, { ref: "origin/main", commit: "b".repeat(40) });
  assert.equal(report.summary.removableFiles, 3);
  assert.equal(report.summary.removableSymbols, 2);
  assert.equal(report.summary.removableLines, 132);
  assert.equal(report.summary.analyzedLines, 230);
  assert.equal(report.summary.deadWeightBasisPoints, 5739);
  assert.deepEqual(report.preexisting, { symbols: 1, files: 1, lines: 5 });
  assert.equal(report.protected[0].reason, "public-api");

  const billing = report.groups.find((group) => group.directory === "src/features/billing");
  assert.ok(billing);
  const byId = new Map(billing.items.map((item) => [item.id, item]));
  assert.equal(
    byId.get("symbol:src/features/billing/format.ts#formatInvoice").trigger.kind,
    "removed-import",
  );
  assert.equal(
    byId.get("symbol:src/features/billing/format.ts#formatInvoice").trigger.file,
    "src/features/billing/InvoiceCard.tsx",
  );
  assert.equal(byId.get("symbol:src/features/billing/totals.ts#totals").trigger.kind, "indirect");
  assert.equal(byId.get("symbol:src/features/billing/totals.ts#totals").trigger.certainty, "certain");

  const lib = report.groups.find((group) => group.directory === "src/lib");
  assert.ok(lib);
  assert.deepEqual(lib.items.map((item) => item.kind), ["file"]);
  assert.equal(lib.items[0].classification, "branch-orphan");
  assert.equal(firstWhySymbol(report), "src/features/billing/format.ts#formatInvoice");
});

test("is byte-identical across repeated builds", () => {
  const files = [file("src/a.ts", { lines: 10 })];
  const symbols = [symbol("src/a.ts#dead", { lines: 4 })];
  const snap = snapshot(files, symbols, { candidates: ["src/a.ts#dead"] });
  const first = JSON.stringify(buildCurrentReport(currentInput(snap)));
  const second = JSON.stringify(buildCurrentReport(currentInput(snap)));
  assert.equal(first, second);
});

test("normalizes engine protection strings to the plan vocabulary", () => {
  assert.equal(normalizeProtectedReason("blocked by public API"), "public-api");
  assert.equal(normalizeProtectedReason("blocked by generated file"), "generated-file");
  assert.equal(normalizeProtectedReason("entry"), "entry-role");
  assert.equal(normalizeProtectedReason("probable edge"), "probable-edge");
  assert.equal(normalizeProtectedReason("unknown edge"), "unknown-edge");
  assert.equal(normalizeProtectedReason("unparsed source"), "unparsed-file");
  assert.equal(normalizeProtectedReason("test entry"), "test-entry");
  assert.equal(normalizeProtectedReason("external referrer"), "external-referrer");
  assert.equal(normalizeProtectedReason("blast radius"), "blast-radius");
  assert.equal(normalizeProtectedReason("workspace boundary"), "workspace-boundary");
  assert.equal(normalizeProtectedReason("oversized batch"), "oversized");
  assert.equal(normalizeProtectedReason("policy"), "policy");
  assert.equal(normalizeProtectedReason("something new"), "other");
  assert.equal(normalizeProtectedReason("no witness; isolated symbol"), null);
});

test("sorts dependencies by package, name, kind, then directory", () => {
  const items = dependencyItems({
    packages: [
      {
        name: "web",
        directory: "apps/web",
        unused: [{ name: "zod", kind: "dependencies", reason: "unused" }],
        unlisted: [{ name: "left-pad", importedBy: ["src/a.ts"], specifiers: ["left-pad"] }],
        transitive: [],
        counts: { dependencies: 1, devDependencies: 0, used: 0 },
      },
      {
        name: "api",
        directory: "apps/api",
        unused: [{ name: "react", kind: "dependencies", reason: "unused" }],
        unlisted: [],
        transitive: [],
        counts: { dependencies: 1, devDependencies: 0, used: 0 },
      },
    ],
    summary: {
      packages: 2,
      unused: 2,
      unlisted: 1,
      transitive: 0,
      unusedByKind: {
        dependencies: 2,
        devDependencies: 0,
        optionalDependencies: 0,
        peerDependencies: 0,
      },
    },
  });

  assert.deepEqual(items.map((item) => item.package), ["api", "web", "web"]);
  assert.deepEqual(
    items.filter((item) => item.package === "web").map((item) => item.name),
    ["left-pad", "zod"],
  );
  assert.ok(items.every((item) => item.advisory === true));
});

test("caps diagnostic groups at six and reports totals", () => {
  const diagnostics = [];
  for (let index = 0; index < 9; index += 1) {
    diagnostics.push({
      code: `code-${index}`,
      severity: "warning",
      message: `diagnostic ${index}`,
      file: `src/file-${index}.ts`,
    });
  }
  const summary = summarizeDiagnostics(diagnostics);
  const capped = reportDiagnostics(summary);
  assert.equal(capped.total, 9);
  assert.equal(capped.groups.length, 6);
});
