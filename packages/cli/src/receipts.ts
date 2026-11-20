import {
  buildReceipt, captureEnvironment, toReceiptChecks,
  type ReceiptBuild, type ReceiptRejection,
} from "@shrike/receipts";
import {
  bisectFailure, classifyCheck, detectPackageManager, runChecks,
  type Baseline, type BisectUnit, type CheckResult, type CheckSpec, type ComparedCheck,
} from "@shrike/verifier";
import { restoreAll } from "./git.ts";
import type { RecoveryReference } from "./recovery.ts";
import { UsageError } from "./errors.ts";
import { VERSION } from "./version.ts";
import type { ShrikeConfig, VerificationTier } from "./config.ts";
import type { Plan, Snapshot } from "./cli-types.ts";
import type { AnalysisPair } from "./session.ts";
import { applySymbols } from "./apply.ts";

export interface BatchLike {
  id: string;
  symbols: string[];
  files: string[];
  lines: number;
}

export function makeReceipt(
  root: string,
  pair: AnalysisPair,
  plan: Plan,
  batch: BatchLike,
  checks: ComparedCheck[],
  outcome: string,
  patchPath: string,
  rejected: ReceiptRejection[] = [],
  recovery?: RecoveryReference,
  build?: ReceiptBuild,
) {
  const policy = plan.policy;
  return buildReceipt({
    tool: { name: "shrike", version: VERSION },
    base: { ref: pair.baseRef, commit: pair.baseCommit, graphHash: pair.base.graphHash },
    head: { commit: pair.headCommit, graphHash: pair.head.graphHash, dirty: pair.dirty },
    plan: { id: plan.id, hash: plan.deltaHash },
    batch: { id: batch.id, symbols: batch.symbols, files: batch.files, linesRemoved: batch.lines, patches: [patchPath || `.shrike/patches/${batch.id}.patch`] },
    checks: toReceiptChecks(checks, { root }),
    outcome: outcome as never,
    rejected,
    policy: {
      entryPolicy: (policy?.entryPolicy as "local" | "production" | undefined) ?? "local",
      maxSymbols: policy?.maxSymbols ?? 50,
      maxFiles: policy?.maxFiles ?? 10,
      allowOversized: policy?.allowOversized ?? false,
    },
    environment: captureEnvironment({ packageManager: detectPackageManager(root) }),
    ...(recovery ? { recovery } : {}),
    ...(build ? { build } : {}),
  });
}

export function blockingChecks(checks: ComparedCheck[]): ComparedCheck[] {
  return checks.filter((check) => check.required && ["new-failure", "fail", "timeout", "error", "flaky"].includes(check.classification ?? ""));
}

export function rejectionsFor(symbols: string[], files: string[], blocking: ComparedCheck[], reason: string): ReceiptRejection[] {
  return blocking.map((check) => ({
    symbols,
    files,
    check: check.name,
    signature: check.signature ?? `status:${check.status}`,
    reason,
  }));
}

export function subsetBatch(snapshot: Snapshot, batch: BatchLike, symbols: string[], id: string): BatchLike {
  const files = [...new Set(symbols
    .map((symbol) => snapshot.symbols.find((candidate) => candidate.key === symbol)?.file)
    .filter((file): file is string => Boolean(file)))];
  const lines = symbols.reduce((total, symbol) => total + (snapshot.symbols.find((candidate) => candidate.key === symbol)?.lines ?? 1), 0);
  return { id, symbols, files: files.length > 0 ? files : batch.files, lines };
}

export async function bisectBatch(
  workingRoot: string,
  snapshot: Snapshot,
  batch: BatchLike,
  checks: ShrikeConfig["checks"],
  baseline: Baseline,
  checksEnabled: boolean,
): Promise<{ status: string; rejected: string[]; compared: ComparedCheck[]; reason: string }> {
  const units: BisectUnit[] = batch.symbols
    .map((symbol) => ({ id: symbol, files: [snapshot.symbols.find((candidate) => candidate.key === symbol)?.file ?? ""] }))
    .filter((unit) => unit.files[0] !== "");
  let lastCompared: ComparedCheck[] = [];
  let lastReason = "";
  const result = await bisectFailure(units, async (subset) => {
    restoreAll(workingRoot);
    try {
      const lines = applySymbols(workingRoot, snapshot, subset.map((unit) => unit.id));
      if (lines === 0 || !checksEnabled) return true;
      const after = await runChecks(checks, { cwd: workingRoot });
      const compared: ComparedCheck[] = after.map((check: CheckResult) => {
        const base = baseline.checks.find((candidate) => candidate.name === check.name);
        return { ...check, baselineStatus: base?.status ?? "missing", baselineSignature: base?.signature ?? null, classification: classifyCheck(check, base) };
      });
      lastCompared = compared;
      const blocking = blockingChecks(compared);
      if (blocking.length > 0) lastReason = blocking.map((check) => check.name).join(", ");
      return blocking.length === 0;
    } finally {
      restoreAll(workingRoot);
    }
  }, { maxAttempts: 16 });
  return { status: result.status, rejected: result.rejected.map((unit) => unit.id), compared: lastCompared, reason: result.reason || lastReason || "check regression" };
}
export function parseTier(value: string | undefined, fallback: VerificationTier): VerificationTier {
  if (value === undefined) return fallback;
  if (value === "fast" || value === "standard" || value === "full") return value;
  throw new UsageError(`invalid check tier: ${value}; expected fast, standard, or full`);
}
