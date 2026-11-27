import test from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { resolve } from "node:path";

import Ajv2020 from "ajv/dist/2020.js";

import {
  buildCurrentDelta,
  buildCurrentPlan,
  recountCurrentPlan,
} from "../src/current-state.ts";

const root = resolve(fileURLToPath(new URL("../../..", import.meta.url)));
const hasEngine = existsSync(
  process.env.SHRIKE_ENGINE ?? resolve(root, "target", "debug", "shrike-engine"),
) || existsSync(resolve(root, "target", "release", "shrike-engine"));

const schemaPath = fileURLToPath(new URL("../../../schemas/delta.schema.json", import.meta.url));
const schema = JSON.parse(readFileSync(schemaPath, "utf8"));
const validateDelta = new Ajv2020({ allErrors: true, strict: false }).compile(schema);

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

function snapshot(files, symbols, overrides = {}) {
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
    graphHash: `blake3:${"0".repeat(64)}`,
    ...overrides,
  };
}

function plan(overrides = {}) {
  return {
    schemaVersion: "1.0",
    id: "plan_fixture",
    deltaHash: "blake3:0",
    base: { ref: null, commit: "a".repeat(40) },
    head: { commit: "a".repeat(40), dirty: false },
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

test("builds a schema-valid synthetic delta from current candidates and unused files", () => {
  const files = [
    file("src/dead.ts", { lines: 20 }),
    file("src/orphan.ts", { lines: 15 }),
    file("src/generated.ts", { lines: 999, generated: true }),
  ];
  const symbols = [
    symbol("src/dead.ts#dead", { lines: 8 }),
    symbol("src/orphan.ts#__module__", { kind: "module", lines: 1 }),
  ];
  const snap = snapshot(files, symbols, {
    candidates: ["src/dead.ts#dead"],
    unusedFiles: ["src/orphan.ts", "src/generated.ts"],
  });

  const delta = buildCurrentDelta(snap, "a".repeat(40), true);

  assert.equal(validateDelta(delta), true, JSON.stringify(validateDelta.errors));
  assert.deepEqual(delta.base, { ref: null, commit: "a".repeat(40) });
  assert.equal(delta.head.dirty, true);
  assert.equal(delta.head.graphHash, `blake3:${"0".repeat(64)}`);
  assert.deepEqual(delta.newlyDead.map((item) => item.symbol), [
    "src/dead.ts#dead",
    "src/orphan.ts#__module__",
  ]);
  assert.deepEqual(delta.newlyDead[0].trigger, {
    file: "src/dead.ts",
    kind: "indirect",
    change: "dead at head with no witness from any entry",
    certainty: "certain",
  });
  assert.deepEqual(delta.newlyDeadFiles, ["src/orphan.ts"]);
  assert.deepEqual(delta.cascade, { symbols: 2, files: 2, lines: 23 });
  assert.deepEqual(delta.batches, []);
});

test("adds a missing whole file to the batch for its directory and recounts", () => {
  const files = [
    file("src/features/billing/format.ts", { lines: 20 }),
    file("src/features/billing/dead.ts", { lines: 30 }),
  ];
  const symbols = [
    symbol("src/features/billing/format.ts#formatInvoice", { lines: 12 }),
    symbol("src/features/billing/dead.ts#a", { lines: 10 }),
    symbol("src/features/billing/dead.ts#b", { lines: 10 }),
  ];
  const snap = snapshot(files, symbols, { unusedFiles: ["src/features/billing/dead.ts"] });
  const source = plan({
    batches: [
      {
        id: "batch_1",
        symbols: ["src/features/billing/format.ts#formatInvoice"],
        files: ["src/features/billing/format.ts"],
        lines: 12,
        patch: ".shrike/patches/batch_1.patch",
      },
    ],
    totals: { symbols: 1, files: 1, lines: 12 },
  });

  const result = recountCurrentPlan(source, snap);

  assert.equal(result.batches.length, 1);
  assert.deepEqual(result.batches[0].symbols, [
    "src/features/billing/dead.ts#__module__",
    "src/features/billing/format.ts#formatInvoice",
  ]);
  assert.deepEqual(result.batches[0].files, [
    "src/features/billing/dead.ts",
    "src/features/billing/format.ts",
  ]);
  assert.equal(result.batches[0].lines, 42);
  assert.deepEqual(result.totals, { symbols: 2, files: 2, lines: 42 });
  // The input plan is not mutated.
  assert.equal(source.batches[0].symbols.length, 1);
  assert.equal(source.totals.lines, 12);
});

test("creates a deterministic batch when no existing batch shares a directory", () => {
  const files = [
    file("src/features/billing/dead.ts", { lines: 30 }),
    file("src/lib/old.ts", { lines: 40 }),
  ];
  const symbols = [
    symbol("src/features/billing/dead.ts#x", { lines: 10 }),
    symbol("src/features/billing/dead.ts#y", { lines: 10 }),
    symbol("src/lib/old.ts#z", { lines: 10 }),
  ];
  const snap = snapshot(files, symbols, {
    unusedFiles: ["src/features/billing/dead.ts", "src/lib/old.ts"],
  });
  const source = plan({
    batches: [
      {
        id: "batch_1",
        symbols: ["src/lib/old.ts#unrelated"],
        files: ["src/lib/old.ts"],
        lines: 1,
        patch: ".shrike/patches/batch_1.patch",
      },
    ],
  });
  symbols.push(symbol("src/lib/old.ts#unrelated", { lines: 1 }));

  const result = recountCurrentPlan(source, snap);

  assert.deepEqual(result.batches.map((batch) => batch.id), ["batch_1", "batch_2"]);
  // batch_1 already owns src/lib/old.ts, so the unused lib file joins it.
  assert.ok(result.batches[0].symbols.includes("src/lib/old.ts#__module__"));
  assert.deepEqual(result.batches[1].symbols, ["src/features/billing/dead.ts#__module__"]);
  assert.equal(result.batches[1].patch, ".shrike/patches/batch_2.patch");
});

test("does not duplicate an existing module key and counts the whole file once", () => {
  const files = [file("src/dead.ts", { lines: 30 })];
  const symbols = [symbol("src/dead.ts#__module__", { kind: "module", lines: 1 })];
  const snap = snapshot(files, symbols, { unusedFiles: ["src/dead.ts"] });
  const source = plan({
    batches: [
      {
        id: "batch_1",
        symbols: ["src/dead.ts#__module__"],
        files: ["src/dead.ts"],
        lines: 30,
        patch: ".shrike/patches/batch_1.patch",
      },
    ],
  });

  const result = recountCurrentPlan(source, snap);

  assert.deepEqual(result.batches[0].symbols, ["src/dead.ts#__module__"]);
  assert.equal(result.batches[0].lines, 30);
  assert.deepEqual(result.totals, { symbols: 1, files: 1, lines: 30 });
});

test("ignores unused files that the cleanup layer cannot safely remove", () => {
  const files = [
    file("docs/page.mdx", { moduleKind: "mdx", lines: 100 }),
    file("src/generated.ts", { generated: true, lines: 100 }),
    file("src/broken.ts", { parseStatus: "error", lines: 100 }),
  ];
  const snap = snapshot(files, [], {
    unusedFiles: ["docs/page.mdx", "src/generated.ts", "src/broken.ts"],
  });

  const result = recountCurrentPlan(plan(), snap);

  assert.deepEqual(result.batches, []);
  assert.deepEqual(result.totals, { symbols: 0, files: 0, lines: 0 });
});

test("buildCurrentPlan analyzes once and returns the recounted plan", { skip: !hasEngine }, async () => {
  const { loadConfig } = await import("../src/config.ts");
  const { mkdtempSync, writeFileSync, mkdirSync, rmSync } = await import("node:fs");
  const { execFileSync } = await import("node:child_process");
  const { tmpdir } = await import("node:os");
  const { join } = await import("node:path");
  const root = mkdtempSync(join(tmpdir(), "shrike-current-plan-"));
  try {
    mkdirSync(join(root, "src"), { recursive: true });
    writeFileSync(join(root, "package.json"), JSON.stringify({ name: "fixture", private: true }));
    writeFileSync(join(root, "src", "index.ts"), "export const live = 1;\n");
    writeFileSync(join(root, "src", "dead.ts"), "export function dead() { return 1; }\n");
    execFileSync("git", ["init", "-q"], { cwd: root });
    execFileSync("git", ["config", "user.email", "test@example.com"], { cwd: root });
    execFileSync("git", ["config", "user.name", "Test"], { cwd: root });
    execFileSync("git", ["add", "."], { cwd: root });
    execFileSync("git", ["commit", "-qm", "init"], { cwd: root });
    const config = await loadConfig(root);
    const { plan: built } = await buildCurrentPlan(root, config);
    assert.equal(built.base.ref, null);
    assert.ok(built.batches.every((batch) => batch.symbols.length > 0 && batch.files.length > 0));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
