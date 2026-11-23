/** Receipt types (see docs/architecture.md). */

export type ReceiptOutcome =
  | "verified"
  | "rejected"
  | "known-failure"
  | "flaky"
  | "error"
  | "revert-failed"
  | "interaction-failure";

export interface ReceiptTool {
  name: "shrike";
  version: string;
}

export interface ReceiptBase {
  ref: string;
  commit: string;
  graphHash: string;
}

export interface ReceiptHead {
  commit: string;
  graphHash: string;
  dirty?: boolean;
}

export interface ReceiptPlan {
  id: string;
  hash: string;
}

export interface ReceiptBuild {
  source: string;
  snapshotHash: string;
  version: string;
  status:
    | "full"
    | "partial"
    | "low-coverage"
    | "stale"
    | "malformed"
    | "unsupported"
    | "absent"
    | "build-failed";
}

export interface ReceiptBatch {
  id: string;
  symbols: string[];
  files: string[];
  linesRemoved: number;
  patches: string[];
}

export type ReceiptCheckStatus = "pass" | "fail" | "timeout" | "skipped" | "error";
export type ReceiptBaselineStatus = ReceiptCheckStatus | "unknown";
export type ReceiptClassification =
  | "pass"
  | "improvement"
  | "known-failure"
  | "new-failure"
  | "fail"
  | "timeout"
  | "flaky"
  | "skipped";

export interface ReceiptCheck {
  name: string;
  command: string;
  status: ReceiptCheckStatus;
  baselineStatus: ReceiptBaselineStatus;
  durationMs: number;
  reruns: number;
  required?: boolean;
  classification?: ReceiptClassification;
  failureSignature?: string;
}

export interface ReceiptRejection {
  symbols: string[];
  files?: string[];
  check: string;
  signature: string;
  reason?: string;
}

export interface ReceiptPolicy {
  entryPolicy: "local" | "production" | "ci";
  maxSymbols: number;
  maxFiles: number;
  allowOversized: boolean;
  maxLiveGraphPercent?: number;
}

export interface ReceiptEnvironment {
  node: string;
  packageManager: string;
  platform: string;
  os?: string;
  arch?: string;
  shrike?: string;
}

export interface ReceiptRecovery {
  snapshotPath: string;
  restoreCommand: string;
}

export interface Receipt {
  schemaVersion: "1.0";
  id: string;
  createdAt: string;
  /** SHA-256 of the canonical receipt excluding createdAt, id, and this field. */
  contentHash: string;
  tool: ReceiptTool;
  base: ReceiptBase;
  head: ReceiptHead;
  plan: ReceiptPlan;
  build?: ReceiptBuild;
  batch: ReceiptBatch;
  checks: ReceiptCheck[];
  outcome: ReceiptOutcome;
  rejected: ReceiptRejection[];
  policy: ReceiptPolicy;
  environment: ReceiptEnvironment;
  recovery?: ReceiptRecovery;
}

/** Input accepted by `buildReceipt`; id, hash, and timestamp are derived. */
export interface ReceiptInput {
  tool: ReceiptTool;
  base: ReceiptBase;
  head: ReceiptHead;
  plan: ReceiptPlan;
  build?: ReceiptBuild;
  batch: ReceiptBatch;
  checks: ReceiptCheck[];
  outcome: ReceiptOutcome;
  rejected?: ReceiptRejection[];
  policy: ReceiptPolicy;
  environment: ReceiptEnvironment;
  recovery?: ReceiptRecovery;
  createdAt?: string;
}

export interface ReceiptVerification {
  ok: boolean;
  expectedHash: string | null;
  expectedId: string | null;
  problems: string[];
}
