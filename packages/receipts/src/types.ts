/** Receipt types for docs/05-verified-deletion.md §4 and §4.2. */

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
  graphHash?: string;
}

export interface ReceiptHead {
  commit: string;
  graphHash?: string;
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
  status: "full" | "partial" | "none";
}

export interface ReceiptBatch {
  id: string;
  symbols: string[];
  files: string[];
  linesRemoved: number;
  patches: string[];
}

export interface ReceiptCheck {
  name: string;
  command: string;
  status: string;
  baselineStatus?: string;
  durationMs: number;
  reruns: number;
  classification?: string;
  signature?: string;
  baselineSignature?: string;
  required?: boolean;
}

export interface ReceiptRejection {
  symbols: string[];
  check?: string;
  signature?: string;
  reason: string;
}

export interface ReceiptPolicy {
  entryPolicy?: "local" | "production";
  includeGenerated?: boolean;
  maxSymbols?: number;
  maxFiles?: number;
  maxLiveGraphPercent?: number;
  allowOversized?: boolean;
}

export interface ReceiptEnvironment {
  node: string;
  packageManager: string;
  platform: string;
  arch?: string;
  ci?: boolean;
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
  policy?: ReceiptPolicy;
  environment: ReceiptEnvironment;
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
  policy?: ReceiptPolicy;
  environment: ReceiptEnvironment;
  createdAt?: string;
}

export interface ReceiptVerification {
  ok: boolean;
  expectedHash: string | null;
  expectedId: string | null;
  problems: string[];
}
