import type { Receipt } from "../types.ts";
import { GRAPH_HASH, HASH, OUTCOMES, RECEIPT_ID, type ReceiptValidation } from "./constants.ts";
import { exactKeys, isRecord } from "./primitives.ts";
import {
  validateBase, validateBatch, validateBuild, validateChecks, validateEnvironment, validateHead,
  validatePlan, validatePolicy, validateRejected, validateRecovery, validateTool,
} from "./sections.ts";

/** Validate a receipt against the published schema's structural contract. */
export function validateReceipt(value: unknown): ReceiptValidation {
  const problems: string[] = [];
  if (!isRecord(value)) return { ok: false, problems: ["receipt must be an object"] };

  exactKeys(
    value,
    [
      "schemaVersion",
      "id",
      "createdAt",
      "tool",
      "base",
      "head",
      "plan",
      "batch",
      "checks",
      "outcome",
      "rejected",
      "policy",
      "environment",
      "contentHash",
    ],
    ["build", "recovery"],
    "receipt",
    problems,
  );

  if (value.schemaVersion !== "1.0") {
    problems.push("schemaVersion must be 1.0");
  }
  if (typeof value.id !== "string" || !RECEIPT_ID.test(value.id)) {
    problems.push("id must match rcpt_[A-Za-z0-9]{8,64}");
  }
  if (typeof value.createdAt !== "string" || Number.isNaN(Date.parse(value.createdAt))) {
    problems.push("createdAt must be a date-time string");
  }
  validateTool(value.tool, problems);
  validateBase(value.base, problems);
  validateHead(value.head, problems);
  validatePlan(value.plan, problems);
  if ("build" in value) validateBuild(value.build, problems);
  validateBatch(value.batch, problems);
  validateChecks(value.checks, problems);
  if (typeof value.outcome !== "string" || !OUTCOMES.has(value.outcome)) {
    problems.push("outcome is not a known outcome");
  }
  validateRejected(value.rejected, problems);
  validatePolicy(value.policy, problems);
  validateEnvironment(value.environment, problems);
  if ("recovery" in value) validateRecovery(value.recovery, problems);
  if (typeof value.contentHash !== "string" || !HASH.test(value.contentHash)) {
    problems.push("contentHash must be sha256:hex");
  }

  if (value.outcome === "verified") {
    if (!Array.isArray(value.checks) || value.checks.length === 0) {
      problems.push("a verified receipt must have at least one check");
    }
    if (Array.isArray(value.rejected) && value.rejected.length > 0) {
      problems.push("a verified receipt must not have a rejected set");
    }
  }
  if (value.outcome === "revert-failed" && !("recovery" in value)) {
    problems.push("a revert-failed receipt must include recovery information");
  }

  return { ok: problems.length === 0, problems };
}

/** Narrow a validated value to a Receipt. */
export function asValidReceipt(value: unknown): Receipt {
  const result = validateReceipt(value);
  if (!result.ok) {
    throw new TypeError(`invalid receipt: ${result.problems.join("; ")}`);
  }
  return value as Receipt;
}
