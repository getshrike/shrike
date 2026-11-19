import type { Receipt } from "./types.ts";

/**
 * Structural validation for `schemas/receipt.schema.json`.
 *
 * This is intentionally a focused validator for the one schema Shrike writes.
 * It fails closed on unknown keys, missing required fields, bad enums, and
 * hash/id mismatches. The JSON Schema remains the published contract; the
 * validator exists so a receipt write cannot silently drift from it.
 */

export interface ReceiptValidation {
  ok: boolean;
  problems: string[];
}

const OUTCOMES = new Set([
  "verified",
  "rejected",
  "known-failure",
  "flaky",
  "error",
  "revert-failed",
  "interaction-failure",
]);

const CHECK_STATUSES = new Set(["pass", "fail", "timeout", "skipped", "error"]);
const BASELINE_STATUSES = new Set(["pass", "fail", "timeout", "skipped", "error", "unknown"]);
const CLASSIFICATIONS = new Set([
  "pass",
  "improvement",
  "known-failure",
  "new-failure",
  "fail",
  "timeout",
  "flaky",
  "skipped",
]);
const BUILD_STATUSES = new Set([
  "full",
  "partial",
  "low-coverage",
  "stale",
  "malformed",
  "unsupported",
  "absent",
  "build-failed",
]);
const ENTRY_POLICIES = new Set(["production", "local", "ci"]);

const HASH = /^sha256:[0-9a-f]{64}$/;
const GRAPH_HASH = /^(?:sha256|blake3):[0-9a-f]{64}$/;
const RECEIPT_ID = /^rcpt_[A-Za-z0-9]{8,64}$/;
const BATCH_ID = /^batch_[A-Za-z0-9_-]+$/;
const SYMBOL = /^[^#]+#[^#]+$/;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isString(value: unknown, minLength = 1): value is string {
  return typeof value === "string" && value.length >= minLength;
}

function isInteger(value: unknown, minimum = 0): value is number {
  return typeof value === "number" && Number.isInteger(value) && value >= minimum;
}

function isStringArray(value: unknown, minItems = 0): value is string[] {
  return (
    Array.isArray(value) &&
    value.length >= minItems &&
    value.every((item) => typeof item === "string" && item.length > 0)
  );
}

function unique(values: string[]): boolean {
  return new Set(values).size === values.length;
}

function exactKeys(
  value: Record<string, unknown>,
  required: string[],
  optional: string[],
  path: string,
  problems: string[],
): void {
  for (const key of required) {
    if (!(key in value)) problems.push(`${path}.${key} is required`);
  }
  const allowed = new Set([...required, ...optional]);
  for (const key of Object.keys(value)) {
    if (!allowed.has(key)) problems.push(`${path}.${key} is not allowed`);
  }
}

function validateTool(value: unknown, problems: string[]): void {
  if (!isRecord(value)) {
    problems.push("tool must be an object");
    return;
  }
  exactKeys(value, ["name", "version"], [], "tool", problems);
  if (!isString(value.name)) problems.push("tool.name must be a non-empty string");
  if (!isString(value.version)) problems.push("tool.version must be a non-empty string");
}

function validateBase(value: unknown, problems: string[]): void {
  if (!isRecord(value)) {
    problems.push("base must be an object");
    return;
  }
  exactKeys(value, ["ref", "commit", "graphHash"], [], "base", problems);
  if (!isString(value.ref)) problems.push("base.ref must be a non-empty string");
  if (!isString(value.commit)) problems.push("base.commit must be a non-empty string");
  if (typeof value.graphHash !== "string" || !GRAPH_HASH.test(value.graphHash)) {
    problems.push("base.graphHash must be blake3:hex or sha256:hex");
  }
}

function validateHead(value: unknown, problems: string[]): void {
  if (!isRecord(value)) {
    problems.push("head must be an object");
    return;
  }
  exactKeys(value, ["commit", "graphHash"], ["dirty"], "head", problems);
  if (!isString(value.commit)) problems.push("head.commit must be a non-empty string");
  if (typeof value.graphHash !== "string" || !GRAPH_HASH.test(value.graphHash)) {
    problems.push("head.graphHash must be blake3:hex or sha256:hex");
  }
  if ("dirty" in value && typeof value.dirty !== "boolean") {
    problems.push("head.dirty must be a boolean");
  }
}

function validatePlan(value: unknown, problems: string[]): void {
  if (!isRecord(value)) {
    problems.push("plan must be an object");
    return;
  }
  exactKeys(value, ["id", "hash"], [], "plan", problems);
  if (!isString(value.id)) problems.push("plan.id must be a non-empty string");
  if (typeof value.hash !== "string" || !GRAPH_HASH.test(value.hash)) {
    problems.push("plan.hash must be blake3:hex or sha256:hex");
  }
}

function validateBuild(value: unknown, problems: string[]): void {
  if (!isRecord(value)) {
    problems.push("build must be an object");
    return;
  }
  exactKeys(
    value,
    ["source", "snapshotHash", "version", "status"],
    [],
    "build",
    problems,
  );
  if (!isString(value.source)) problems.push("build.source must be a non-empty string");
  if (typeof value.snapshotHash !== "string" || !HASH.test(value.snapshotHash)) {
    problems.push("build.snapshotHash must be sha256:hex");
  }
  if (!isString(value.version)) problems.push("build.version must be a non-empty string");
  if (typeof value.status !== "string" || !BUILD_STATUSES.has(value.status)) {
    problems.push("build.status is not a known status");
  }
}

function validateBatch(value: unknown, problems: string[]): void {
  if (!isRecord(value)) {
    problems.push("batch must be an object");
    return;
  }
  exactKeys(
    value,
    ["id", "symbols", "files", "linesRemoved", "patches"],
    [],
    "batch",
    problems,
  );
  if (typeof value.id !== "string" || !BATCH_ID.test(value.id)) {
    problems.push("batch.id must match batch_[A-Za-z0-9_-]+");
  }
  if (!isStringArray(value.symbols, 1) || !unique(value.symbols)) {
    problems.push("batch.symbols must be a non-empty unique array of strings");
  } else if (!value.symbols.every((symbol) => SYMBOL.test(symbol))) {
    problems.push("batch.symbols entries must look like path#Name");
  }
  if (!isStringArray(value.files, 1) || !unique(value.files)) {
    problems.push("batch.files must be a non-empty unique array of strings");
  } else if (value.files.some((file) => file.includes("#"))) {
    problems.push("batch.files entries must not contain #");
  }
  if (!isInteger(value.linesRemoved)) {
    problems.push("batch.linesRemoved must be a non-negative integer");
  }
  if (!isStringArray(value.patches, 1) || !unique(value.patches)) {
    problems.push("batch.patches must be a non-empty unique array of strings");
  }
}

function validateChecks(value: unknown, problems: string[]): void {
  if (!Array.isArray(value)) {
    problems.push("checks must be an array");
    return;
  }
  for (const [index, check] of value.entries()) {
    const path = `checks[${index}]`;
    if (!isRecord(check)) {
      problems.push(`${path} must be an object`);
      continue;
    }
    exactKeys(
      check,
      ["name", "command", "status", "baselineStatus", "durationMs", "reruns"],
      ["required", "classification", "failureSignature"],
      path,
      problems,
    );
    if (!isString(check.name)) problems.push(`${path}.name must be a non-empty string`);
    if (!isString(check.command)) problems.push(`${path}.command must be a non-empty string`);
    if (typeof check.status !== "string" || !CHECK_STATUSES.has(check.status)) {
      problems.push(`${path}.status is not a known check status`);
    }
    if (typeof check.baselineStatus !== "string" || !BASELINE_STATUSES.has(check.baselineStatus)) {
      problems.push(`${path}.baselineStatus is not a known baseline status`);
    }
    if (!isInteger(check.durationMs)) {
      problems.push(`${path}.durationMs must be a non-negative integer`);
    }
    if (!isInteger(check.reruns)) {
      problems.push(`${path}.reruns must be a non-negative integer`);
    }
    if ("required" in check && typeof check.required !== "boolean") {
      problems.push(`${path}.required must be a boolean`);
    }
    if (
      "classification" in check &&
      (typeof check.classification !== "string" || !CLASSIFICATIONS.has(check.classification))
    ) {
      problems.push(`${path}.classification is not a known classification`);
    }
    if ("failureSignature" in check && !isString(check.failureSignature)) {
      problems.push(`${path}.failureSignature must be a non-empty string`);
    }
  }
}

function validateRejected(value: unknown, problems: string[]): void {
  if (!Array.isArray(value)) {
    problems.push("rejected must be an array");
    return;
  }
  for (const [index, rejection] of value.entries()) {
    const path = `rejected[${index}]`;
    if (!isRecord(rejection)) {
      problems.push(`${path} must be an object`);
      continue;
    }
    exactKeys(
      rejection,
      ["symbols", "check", "signature"],
      ["files", "reason"],
      path,
      problems,
    );
    if (!isStringArray(rejection.symbols, 1) || !unique(rejection.symbols)) {
      problems.push(`${path}.symbols must be a non-empty unique array`);
    } else if (!rejection.symbols.every((symbol) => SYMBOL.test(symbol))) {
      problems.push(`${path}.symbols entries must look like path#Name`);
    }
    if ("files" in rejection && (!isStringArray(rejection.files) || !unique(rejection.files))) {
      problems.push(`${path}.files must be a unique array of strings`);
    }
    if (!isString(rejection.check)) problems.push(`${path}.check must be a non-empty string`);
    if (!isString(rejection.signature)) {
      problems.push(`${path}.signature must be a non-empty string`);
    }
    if ("reason" in rejection && typeof rejection.reason !== "string") {
      problems.push(`${path}.reason must be a string`);
    }
  }
}

function validatePolicy(value: unknown, problems: string[]): void {
  if (!isRecord(value)) {
    problems.push("policy must be an object");
    return;
  }
  exactKeys(
    value,
    ["entryPolicy", "maxSymbols", "maxFiles", "allowOversized"],
    ["maxLiveGraphPercent"],
    "policy",
    problems,
  );
  if (typeof value.entryPolicy !== "string" || !ENTRY_POLICIES.has(value.entryPolicy)) {
    problems.push("policy.entryPolicy must be production, local, or ci");
  }
  if (!isInteger(value.maxSymbols, 1)) {
    problems.push("policy.maxSymbols must be a positive integer");
  }
  if (!isInteger(value.maxFiles, 1)) {
    problems.push("policy.maxFiles must be a positive integer");
  }
  if (typeof value.allowOversized !== "boolean") {
    problems.push("policy.allowOversized must be a boolean");
  }
  if (
    "maxLiveGraphPercent" in value &&
    (typeof value.maxLiveGraphPercent !== "number" ||
      value.maxLiveGraphPercent < 0 ||
      value.maxLiveGraphPercent > 100)
  ) {
    problems.push("policy.maxLiveGraphPercent must be between 0 and 100");
  }
}

function validateEnvironment(value: unknown, problems: string[]): void {
  if (!isRecord(value)) {
    problems.push("environment must be an object");
    return;
  }
  exactKeys(
    value,
    ["node", "packageManager", "platform"],
    ["os", "arch", "shrike"],
    "environment",
    problems,
  );
  for (const key of ["node", "packageManager", "platform"] as const) {
    if (!isString(value[key])) problems.push(`environment.${key} must be a non-empty string`);
  }
  for (const key of ["os", "arch", "shrike"] as const) {
    if (key in value && typeof value[key] !== "string") {
      problems.push(`environment.${key} must be a string`);
    }
  }
}

function validateRecovery(value: unknown, problems: string[]): void {
  if (!isRecord(value)) {
    problems.push("recovery must be an object");
    return;
  }
  exactKeys(value, ["snapshotPath", "restoreCommand"], [], "recovery", problems);
  if (!isString(value.snapshotPath)) {
    problems.push("recovery.snapshotPath must be a non-empty string");
  }
  if (!isString(value.restoreCommand)) {
    problems.push("recovery.restoreCommand must be a non-empty string");
  }
}

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
