export interface ReceiptValidation {
  ok: boolean;
  problems: string[];
}

export const OUTCOMES = new Set([
  "verified",
  "rejected",
  "known-failure",
  "flaky",
  "error",
  "revert-failed",
  "interaction-failure",
]);

export const CHECK_STATUSES = new Set(["pass", "fail", "timeout", "skipped", "error"]);
export const BASELINE_STATUSES = new Set(["pass", "fail", "timeout", "skipped", "error", "unknown"]);
export const CLASSIFICATIONS = new Set([
  "pass",
  "improvement",
  "known-failure",
  "new-failure",
  "fail",
  "timeout",
  "flaky",
  "skipped",
]);
export const BUILD_STATUSES = new Set([
  "full",
  "partial",
  "low-coverage",
  "stale",
  "malformed",
  "unsupported",
  "absent",
  "build-failed",
]);
export const ENTRY_POLICIES = new Set(["production", "local", "ci"]);

export const HASH = /^sha256:[0-9a-f]{64}$/;
export const GRAPH_HASH = /^(?:sha256|blake3):[0-9a-f]{64}$/;
export const RECEIPT_ID = /^rcpt_[A-Za-z0-9]{8,64}$/;
export const BATCH_ID = /^batch_[A-Za-z0-9_-]+$/;
export const SYMBOL = /^[^#]+#[^#]+$/;
