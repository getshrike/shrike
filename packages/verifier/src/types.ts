/**
 * Verifier types.
 *
 * The verifier owns process execution and result classification. It never
 * decides what to delete; it only answers whether a check passed, failed, was
 * already failing, timed out, or was flaky.
 */

/** Status of a single check execution. */
export type CheckStatus = "pass" | "fail" | "timeout" | "error" | "skipped";

/** Baseline status is a check status plus "missing" when no baseline exists. */
export type BaselineStatus = CheckStatus | "missing";

/** Comparison classification (see docs/architecture.md). */
export type CheckClassification =
  | "pass"
  | "improvement"
  | "known-failure"
  | "new-failure"
  | "fail"
  | "timeout"
  | "flaky"
  | "error"
  | "skipped";

/** Check tiers (see docs/architecture.md). */
export type CheckTier = "fast" | "standard" | "full" | "none";

/** A check the host is allowed to run. */
export interface CheckSpec {
  name: string;
  command: string;
  /** Working directory. Relative paths are resolved by the caller. */
  cwd?: string;
  timeoutMs?: number;
  /** Required checks block a deletion; advisory checks are recorded only. */
  required?: boolean;
  /** Extra attempts after the first failure. Default 0. */
  reruns?: number;
  env?: Record<string, string>;
}

/** Result of one check, including reruns and a stable failure signature. */
export interface CheckResult {
  name: string;
  command: string;
  status: CheckStatus;
  exitCode: number | null;
  durationMs: number;
  /** Normalized, capped output suitable for a receipt or log. */
  output: string;
  /** `sha256:<hex>` for failures, null for passes. */
  signature: string | null;
  timedOut: boolean;
  /** Number of extra attempts actually performed. */
  reruns: number;
  /** True when a retry passed or failure signatures differed between runs. */
  flaky: boolean;
  required: boolean;
}

/** A check result compared with its baseline. */
export interface ComparedCheck extends CheckResult {
  baselineStatus: BaselineStatus;
  baselineSignature: string | null;
  classification: CheckClassification;
}

/** Stored baseline for one head commit and one check configuration. */
export interface Baseline {
  schemaVersion: "1.0";
  head: string;
  capturedAt: string;
  checks: CheckResult[];
  configHash?: string;
}

/** Raw process result before normalization. */
export interface RawCommandResult {
  exitCode: number | null;
  signal: NodeJS.Signals | null;
  stdout: string;
  stderr: string;
  durationMs: number;
  timedOut: boolean;
  spawnError: string | null;
}

/** Options shared by command execution helpers. */
export interface RunOptions {
  cwd?: string;
  env?: NodeJS.ProcessEnv;
  timeoutMs?: number;
  maxOutputBytes?: number;
}

/** Outcome of a verified batch. */
export type BatchOutcome =
  | "verified"
  | "rejected"
  | "known-failure"
  | "flaky"
  | "error"
  | "revert-failed"
  | "interaction-failure";
