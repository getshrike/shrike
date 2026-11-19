import type {
  Baseline,
  BaselineStatus,
  BatchOutcome,
  CheckClassification,
  CheckResult,
  ComparedCheck,
} from "./types.ts";

/**
 * Baseline comparison and classification.
 *
 * The rule is comparison against the baseline, not against zero. A pre-existing
 * failure is `known-failure` and does not block; a different failure is a
 * regression and blocks.
 */

/** Classify one after-result against one baseline-result. */
export function classifyCheck(
  after: CheckResult,
  baseline: CheckResult | undefined,
): CheckClassification {
  const baselineStatus: BaselineStatus = baseline?.status ?? "missing";

  if (after.status === "skipped") return "skipped";
  if (after.flaky) return "flaky";

  if (after.status === "pass") {
    if (baselineStatus === "fail" || baselineStatus === "timeout" || baselineStatus === "error") {
      return "improvement";
    }
    return "pass";
  }

  const sameSignature =
    after.signature !== null &&
    baseline?.signature !== null &&
    baseline?.signature !== undefined &&
    after.signature === baseline.signature;

  if (after.status === "timeout") {
    if (baselineStatus === "timeout" && sameSignature) return "known-failure";
    if (baselineStatus === "fail" && sameSignature) return "known-failure";
    return "timeout";
  }

  if (after.status === "error") {
    return "error";
  }

  // after.status === "fail"
  if (baselineStatus === "fail" || baselineStatus === "timeout" || baselineStatus === "error") {
    return sameSignature ? "known-failure" : "new-failure";
  }
  return "fail";
}

/** Compare a set of after-results to a stored baseline. */
export function compareToBaseline(
  baseline: Baseline,
  after: CheckResult[],
): ComparedCheck[] {
  const byName = new Map<string, CheckResult>();
  for (const check of baseline.checks) {
    byName.set(check.name, check);
  }

  return after.map((result) => {
    const baselineResult = byName.get(result.name);
    return {
      ...result,
      baselineStatus: baselineResult?.status ?? "missing",
      baselineSignature: baselineResult?.signature ?? null,
      classification: classifyCheck(result, baselineResult),
    };
  });
}

/** True when a classification must block a deletion. */
export function isBlocking(classification: CheckClassification): boolean {
  return (
    classification === "new-failure" ||
    classification === "fail" ||
    classification === "timeout" ||
    classification === "error"
  );
}

/** Required checks block; advisory checks are recorded only. */
export function hasBlockingFailure(
  checks: ComparedCheck[],
  options: { requiredOnly?: boolean } = {},
): boolean {
  const requiredOnly = options.requiredOnly ?? true;
  return checks.some((check) => {
    if (requiredOnly && !check.required) return false;
    return isBlocking(check.classification);
  });
}

/**
 * Outcome for a batch whose checks completed.
 *
 * `rejected`, `revert-failed`, `error`, and `interaction-failure` are decided by
 * the caller. This function only distinguishes the successful outcomes.
 */
export function batchOutcome(
  checks: ComparedCheck[],
  options: { requiredOnly?: boolean } = {},
): Extract<BatchOutcome, "verified" | "rejected" | "known-failure" | "flaky" | "error"> {
  const requiredOnly = options.requiredOnly ?? true;
  const considered = checks.filter((check) => !requiredOnly || check.required);

  if (considered.some((check) => check.classification === "error")) return "error";
  if (considered.some((check) => isBlocking(check.classification))) return "rejected";
  if (considered.some((check) => check.classification === "flaky")) return "flaky";
  if (considered.some((check) => check.classification === "known-failure")) {
    return "known-failure";
  }
  return "verified";
}

/** Human-readable one-line summary of a check set. */
export function summarizeChecks(checks: ComparedCheck[]): string {
  if (checks.length === 0) return "no checks";
  return checks
    .map((check) => `${check.name}:${check.classification}`)
    .join(" ");
}

/** Baseline failures are non-blocking only when their signature is stable. */
export function unstableBaseline(baseline: Baseline): CheckResult[] {
  return baseline.checks.filter((check) => check.flaky);
}
