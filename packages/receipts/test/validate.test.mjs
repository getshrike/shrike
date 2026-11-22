import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { validateReceipt } from "../src/validate.ts";
import { verifyReceipt } from "../src/receipt.ts";

function fixture(name) {
  return JSON.parse(
    readFileSync(new URL(`../../../fixtures/receipts/${name}.json`, import.meta.url), "utf8"),
  );
}

function clone(value) {
  return JSON.parse(JSON.stringify(value));
}

test("published receipt fixtures validate and verify", () => {
  for (const name of ["verified", "rejected", "revert-failed"]) {
    const receipt = fixture(name);
    const validation = validateReceipt(receipt);
    assert.equal(validation.ok, true, `${name}: ${validation.problems.join("; ")}`);
    const verification = verifyReceipt(receipt);
    assert.equal(verification.ok, true, `${name}: ${verification.problems.join("; ")}`);
  }
});

test("failureSignature is accepted and legacy signature is rejected", () => {
  const receipt = fixture("rejected");
  assert.equal(validateReceipt(receipt).ok, true);

  const legacy = clone(receipt);
  legacy.checks[0].signature = legacy.checks[0].failureSignature;
  delete legacy.checks[0].failureSignature;
  const validation = validateReceipt(legacy);
  assert.equal(validation.ok, false);
  assert.ok(validation.problems.some((problem) => problem.includes("checks[0].signature is not allowed")));
});

test("baselineStatus is required on every check", () => {
  const receipt = fixture("verified");
  delete receipt.checks[0].baselineStatus;
  const validation = validateReceipt(receipt);
  assert.equal(validation.ok, false);
  assert.ok(validation.problems.some((problem) => problem.includes("baselineStatus is required")));
});

test("unknown environment keys are rejected", () => {
  const receipt = fixture("verified");
  receipt.environment.ci = "github";
  const validation = validateReceipt(receipt);
  assert.equal(validation.ok, false);
  assert.ok(validation.problems.some((problem) => problem.includes("environment.ci is not allowed")));
});

test("verified receipts require at least one check", () => {
  const receipt = fixture("verified");
  receipt.checks = [];
  const validation = validateReceipt(receipt);
  assert.equal(validation.ok, false);
  assert.ok(validation.problems.some((problem) => problem.includes("at least one check")));
});

test("verified receipts cannot carry a rejected set", () => {
  const receipt = fixture("rejected");
  receipt.outcome = "verified";
  const validation = validateReceipt(receipt);
  assert.equal(validation.ok, false);
  assert.ok(validation.problems.some((problem) => problem.includes("must not have a rejected set")));
});

test("revert-failed receipts require recovery information", () => {
  const receipt = fixture("revert-failed");
  assert.equal(validateReceipt(receipt).ok, true);
  delete receipt.recovery;
  const validation = validateReceipt(receipt);
  assert.equal(validation.ok, false);
  assert.ok(validation.problems.some((problem) => problem.includes("must include recovery")));
});

test("engine blake3 graph hashes are accepted", () => {
  const receipt = fixture("verified");
  const hash = `blake3:${"a".repeat(64)}`;
  receipt.base.graphHash = hash;
  receipt.head.graphHash = hash;
  receipt.plan.hash = hash;
  const validation = validateReceipt(receipt);
  assert.equal(validation.ok, true, validation.problems.join("; "));
});

test("tampered content hashes are detected", () => {
  const receipt = fixture("verified");
  receipt.batch.linesRemoved += 1;
  const verification = verifyReceipt(receipt);
  assert.equal(verification.ok, false);
  assert.ok(verification.problems.some((problem) => problem.includes("contentHash")));
});
