import assert from "node:assert/strict";
import type { ProtocolResponse } from "./engine.ts";

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** Assert a decoded protocol envelope has the locked shape. */
export function assertProtocolEnvelope(value: unknown): asserts value is ProtocolResponse {
  assert.ok(isRecord(value), "response must be an object");
  assert.equal(value.protocolVersion, "1.0");
  assert.equal(typeof value.id, "string");
  assert.equal(typeof value.ok, "boolean");
  if (value.ok === true) {
    assert.ok("result" in value && value.result !== undefined, "ok response needs result");
    assert.ok(!("error" in value) || value.error === undefined, "ok response must not have error");
  } else {
    assert.ok(isRecord(value.error), "error response needs an error object");
    assert.equal(typeof value.error.code, "string");
    assert.equal(typeof value.error.message, "string");
  }
}

/** Minimal schema-ish snapshot invariant. */
export function assertSnapshotShape(value: unknown): void {
  assert.ok(isRecord(value), "snapshot must be an object");
  for (const key of ["files", "symbols", "edges", "entries"]) {
    assert.ok(key in value, `snapshot is missing ${key}`);
  }
}

/** Minimal delta invariants (see docs/architecture.md). */
export function assertDeltaShape(value: unknown): void {
  assert.ok(isRecord(value), "delta must be an object");
  assert.equal(value.schemaVersion, "1.0");
  assert.ok(isRecord(value.base), "delta.base must be an object");
  assert.ok(isRecord(value.head), "delta.head must be an object");
  assert.ok(Array.isArray(value.newlyDead), "delta.newlyDead must be an array");
  assert.ok(Array.isArray(value.newlyDeadFiles), "delta.newlyDeadFiles must be an array");
  assert.ok(isRecord(value.cascade), "delta.cascade must be an object");
  assert.ok(Array.isArray(value.batches), "delta.batches must be an array");
  assert.ok(Array.isArray(value.diagnostics), "delta.diagnostics must be an array");
}

/** Minimal deletion-plan invariants. */
export function assertPlanShape(value: unknown): void {
  assert.ok(isRecord(value), "plan must be an object");
  assert.equal(value.schemaVersion, "1.0");
  assert.equal(typeof value.id, "string");
  assert.ok(isRecord(value.base), "plan.base must be an object");
  assert.ok(isRecord(value.head), "plan.head must be an object");
  assert.ok(Array.isArray(value.batches), "plan.batches must be an array");
  assert.ok(Array.isArray(value.blocked), "plan.blocked must be an array");
  for (const batch of value.batches) {
    assert.ok(isRecord(batch), "batch must be an object");
    assert.equal(typeof batch.id, "string");
    assert.ok(Array.isArray(batch.symbols), "batch.symbols must be an array");
    assert.ok(Array.isArray(batch.files), "batch.files must be an array");
    assert.equal(typeof batch.patch, "string");
  }
}

/** Assert a receipt-like object has the fields the host depends on. */
export function assertReceiptShape(value: unknown): void {
  assert.ok(isRecord(value), "receipt must be an object");
  assert.equal(value.schemaVersion, "1.0");
  assert.equal(typeof value.id, "string");
  assert.ok(String(value.id).startsWith("rcpt_"));
  assert.equal(typeof value.contentHash, "string");
  assert.ok(String(value.contentHash).startsWith("sha256:"));
  assert.ok(isRecord(value.batch), "receipt.batch must be an object");
  assert.ok(Array.isArray(value.checks), "receipt.checks must be an array");
  assert.equal(typeof value.outcome, "string");
}
