#!/usr/bin/env node
/**
 * Receipt contract gate.
 *
 * Builds one receipt per outcome, validates the structural schema, verifies the
 * content hash and id, proves reproducibility, validates the schema examples,
 * and validates any committed fixture receipts.
 */
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import {
  buildReceipt,
  hashCanonical,
  stableStringify,
  validateReceipt,
  verifyReceipt,
} from "../packages/receipts/src/index.ts";

const root = resolve(new URL("..", import.meta.url).pathname);
const fixtureDir = join(root, "fixtures", "receipts");
const write = process.argv.includes("--write");
const failures = [];

const hash = (value) => hashCanonical(value);
const createdAt = "2026-11-20T00:00:00.000Z";
const tool = { name: "shrike", version: "1.0.0" };
const base = { ref: "origin/main", commit: "abc123", graphHash: hash({ base: 1 }) };
const head = { commit: "def456", graphHash: hash({ head: 1 }), dirty: false };
const plan = { id: "plan_fixture", hash: hash({ plan: 1 }) };
const policy = { entryPolicy: "local", maxSymbols: 50, maxFiles: 10, allowOversized: false };
const environment = {
  node: "v24.0.0",
  packageManager: "pnpm@12.18.0",
  platform: "linux",
  os: "linux",
  arch: "x64",
  shrike: "1.0.0",
};

const verifiedCheck = {
  name: "typecheck",
  command: "pnpm typecheck",
  status: "pass",
  baselineStatus: "pass",
  durationMs: 812,
  reruns: 0,
  required: true,
  classification: "pass",
};
const failedCheck = {
  name: "test",
  command: "pnpm test",
  status: "fail",
  baselineStatus: "pass",
  durationMs: 1200,
  reruns: 1,
  required: true,
  classification: "fail",
  failureSignature: hash({ failure: 1 }),
};
const batch = {
  id: "batch_1",
  symbols: ["src/lib/format.ts#formatInvoice"],
  files: ["src/lib/format.ts"],
  linesRemoved: 18,
  patches: [".shrike/patches/batch_1.patch"],
};

const receipts = {
  verified: buildReceipt({
    tool,
    base,
    head,
    plan,
    batch,
    checks: [verifiedCheck],
    outcome: "verified",
    rejected: [],
    policy,
    environment,
    createdAt,
  }),
  rejected: buildReceipt({
    tool,
    base,
    head,
    plan,
    batch,
    checks: [failedCheck],
    outcome: "rejected",
    rejected: [
      {
        symbols: batch.symbols,
        files: batch.files,
        check: "test",
        signature: hash({ failure: 1 }),
        reason: "invoice fixture failed after removal",
      },
    ],
    policy,
    environment,
    createdAt,
  }),
  "revert-failed": buildReceipt({
    tool,
    base,
    head,
    plan,
    batch,
    checks: [verifiedCheck],
    outcome: "revert-failed",
    rejected: [],
    policy,
    environment,
    recovery: {
      snapshotPath: ".shrike/recovery/rec_fixture.json",
      restoreCommand: "shrike recover .shrike/recovery/rec_fixture.json",
    },
    createdAt,
  }),
};

for (const [name, receipt] of Object.entries(receipts)) {
  const validation = validateReceipt(receipt);
  if (!validation.ok) {
    failures.push(`${name}: ${validation.problems.join("; ")}`);
    continue;
  }
  const verification = verifyReceipt(receipt);
  if (!verification.ok) failures.push(`${name}: ${verification.problems.join("; ")}`);
}

// createdAt is metadata: the same payload reproduces the id.
const reproduced = buildReceipt({
  tool,
  base,
  head,
  plan,
  batch,
  checks: [verifiedCheck],
  outcome: "verified",
  rejected: [],
  policy,
  environment,
  createdAt: "2030-01-01T00:00:00.000Z",
});
if (reproduced.id !== receipts.verified.id) {
  failures.push("verified receipt id is not reproducible across createdAt values");
}

// The published examples must satisfy the same structural contract.
const schema = JSON.parse(readFileSync(join(root, "schemas", "receipt.schema.json"), "utf8"));
for (const [index, example] of (schema.examples ?? []).entries()) {
  const validation = validateReceipt(example);
  if (!validation.ok) failures.push(`schema example ${index}: ${validation.problems.join("; ")}`);
}

if (write) {
  mkdirSync(fixtureDir, { recursive: true });
  for (const [name, receipt] of Object.entries(receipts)) {
    writeFileSync(join(fixtureDir, `${name}.json`), `${stableStringify(receipt, 2)}\n`, "utf8");
  }
}

// Every committed receipt fixture is validated, not just the generated ones.
if (existsSync(fixtureDir)) {
  for (const name of readdirSync(fixtureDir).filter((value) => value.endsWith(".json")).sort()) {
    try {
      const parsed = JSON.parse(readFileSync(join(fixtureDir, name), "utf8"));
      const validation = validateReceipt(parsed);
      if (!validation.ok) failures.push(`fixtures/receipts/${name}: ${validation.problems.join("; ")}`);
      const verification = verifyReceipt(parsed);
      if (!verification.ok) failures.push(`fixtures/receipts/${name}: ${verification.problems.join("; ")}`);
    } catch (error) {
      failures.push(`fixtures/receipts/${name}: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
}

// Receipts produced by local e2e runs are validated when present.
const localDir = join(root, ".shrike", "receipts");
if (existsSync(localDir)) {
  for (const name of readdirSync(localDir).filter((value) => value.endsWith(".json")).sort()) {
    try {
      const parsed = JSON.parse(readFileSync(join(localDir, name), "utf8"));
      const validation = validateReceipt(parsed);
      if (!validation.ok) failures.push(`.shrike/receipts/${name}: ${validation.problems.join("; ")}`);
      const verification = verifyReceipt(parsed);
      if (!verification.ok) failures.push(`.shrike/receipts/${name}: ${verification.problems.join("; ")}`);
    } catch (error) {
      failures.push(`.shrike/receipts/${name}: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
}

if (failures.length > 0) {
  for (const failure of failures) console.error(`FAIL ${failure}`);
  process.exit(1);
}
console.log(`OK: ${Object.keys(receipts).length} generated receipts, ${(schema.examples ?? []).length} schema examples, and committed/local receipts validate`);
