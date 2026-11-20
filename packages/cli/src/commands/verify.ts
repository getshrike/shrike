import { join } from "node:path";

import { booleanFlag, rejectUnknownFlags, stringFlag, type ParsedArgs } from "../args.ts";
import { loadConfig } from "../config.ts";
import { CliError, ExitCode, RevertFailedError, UsageError } from "../errors.ts";
import { formatCount, writeHuman, writeJson } from "../output.ts";
import { commitAll, createWorktree, headCommit, isDirty, restoreAll } from "../git.ts";
import {
  classifyCheck, detectRepositoryChecks, runChecks,
  type CheckResult, type ComparedCheck,
} from "@shrike/verifier";
import { readReceipt, writeReceipt, type ReceiptRejection } from "@shrike/receipts";
import { analyzePair, toReceiptBuild } from "../session.ts";
import { readStoredPlan } from "../plan-store.ts";
import { loadOrCaptureBaseline } from "../baseline.ts";
import {
  blockingChecks, makeReceipt, parseTier, rejectionsFor, type BatchLike,
} from "../receipts.ts";
import { applySymbols, writePatch } from "../apply.ts";
import { writeRecoverySnapshot, type RecoveryReference } from "../recovery.ts";

export async function verifyCommand(root: string, args: ParsedArgs): Promise<number> {
  const id = stringFlag(args, "plan") ?? args.positionals[0];
  if (!id) throw new UsageError("verify requires --plan <id> or a receipt id");
  // Receipt IDs remain accepted for `shrike verify <rcpt_...>` so an existing
  // receipt can be inspected after the fact. A plan ID re-runs verification.
  if (id.startsWith("rcpt_")) {
    const receipt = readReceipt(join(root, ".shrike", "receipts", `${id}.json`));
    if (booleanFlag(args, "json")) writeJson(receipt);
    else writeHuman(`Receipt ${receipt.id}: ${receipt.outcome}`);
    return receipt.outcome === "verified" ? ExitCode.OK : ExitCode.VERIFICATION_FAILED;
  }
  rejectUnknownFlags(args, ["plan", "json", "help", "in-place", "allow-dirty", "quiet", "tier"]);
  const artifact = readStoredPlan(root, id);
  const config = await loadConfig(root);
  // In-place verification runs checks where dependencies, generated files, and
  // untracked environment files already exist. A git worktree cannot run those
  // checks reliably, so honor both the flag and the configured mode.
  const inPlace = booleanFlag(args, "in-place") || config.verification.mode === "in-place";
  const allowDirty = booleanFlag(args, "allow-dirty") || config.verification.allowDirty;
  if (isDirty(root) && (inPlace || !allowDirty)) {
    throw new UsageError(
      inPlace
        ? "in-place verification requires a clean working tree"
        : "verify requires a clean working tree",
    );
  }
  if (artifact.headDirty) {
    throw new CliError(
      ExitCode.COULD_NOT_RUN,
      "dirty-plan",
      `plan ${artifact.plan.id} was computed on a dirty tree; commit the changes and re-plan before verifying`,
    );
  }
  const current = headCommit(root);
  if (current !== artifact.headCommit) {
    throw new CliError(
      ExitCode.COULD_NOT_RUN,
      "plan-head-mismatch",
      `plan ${artifact.plan.id} targets ${artifact.headCommit.slice(0, 12)}, but HEAD is ${current.slice(0, 12)}`,
    );
  }
  const tier = parseTier(stringFlag(args, "tier"), artifact.tier ?? config.verification.tier);
  const pairArgs: ParsedArgs = {
    positionals: [],
    values: new Map([["base", artifact.baseRef ?? artifact.baseCommit]]),
  };
  const pair = await analyzePair(root, pairArgs, { allowDirty: config.verification.allowDirty });
  const receipts: string[] = [];
  try {
    if (pair.baseCommit !== artifact.baseCommit) {
      throw new CliError(
        ExitCode.COULD_NOT_RUN,
        "plan-base-mismatch",
        `plan ${artifact.plan.id} targets base ${artifact.baseCommit.slice(0, 12)}, but the merge base is ${pair.baseCommit.slice(0, 12)}`,
      );
    }
    const checks = config.checks.length > 0
      ? config.checks
      : detectRepositoryChecks(root, { tier });
    if (checks.length === 0) {
      throw new CliError(
        ExitCode.COULD_NOT_RUN,
        "no-checks",
        "no verification checks were found; configure checks or add typecheck/lint/test scripts",
      );
    }
    const worktree = inPlace ? null : createWorktree(root, artifact.headCommit, "verify");
    const workingRoot = worktree?.directory ?? root;
    let activeRecovery: RecoveryReference | null = null;
    const restore = (): void => {
      try {
        restoreAll(workingRoot);
      } catch (error) {
        throw new RevertFailedError(
          `could not restore ${workingRoot}: ${error instanceof Error ? error.message : String(error)}`,
          activeRecovery?.snapshotPath,
        );
      }
    };
    try {
      const baseline = await loadOrCaptureBaseline(
        root,
        workingRoot,
        artifact.headCommit,
        checks,
        config,
      );
      const compare = async (): Promise<ComparedCheck[]> => {
        const after = await runChecks(checks, { cwd: workingRoot });
        return after.map((result: CheckResult) => {
          const base = baseline.checks.find((check) => check.name === result.name);
          return {
            ...result,
            baselineStatus: base?.status ?? "missing",
            baselineSignature: base?.signature ?? null,
            classification: classifyCheck(result, base),
          };
        });
      };
      const receiptBuild = toReceiptBuild(pair.build);
      const make = (
        batch: BatchLike,
        compared: ComparedCheck[],
        outcome: string,
        patchPath: string,
        rejected: ReceiptRejection[] = [],
      ) => makeReceipt(root, pair, artifact.plan, batch, compared, outcome, patchPath, rejected, undefined, receiptBuild);
      let outcome = "verified";
      let hadRejection = false;
      for (const batch of artifact.plan.batches) {
        restore();
        activeRecovery = writeRecoverySnapshot(root, artifact.headCommit, batch.files);
        const lines = applySymbols(workingRoot, pair.head, batch.symbols);
        if (lines === 0) continue;
        const patchPath = writePatch(root, workingRoot, batch.id);
        const compared = await compare();
        const blocking = blockingChecks(compared);
        if (blocking.length > 0) {
          restore();
          const receipt = make({ ...batch, lines }, compared, "rejected", patchPath, rejectionsFor(batch.symbols, batch.files, blocking, "re-verification regression"));
          receipts.push(writeReceipt(root, receipt));
          outcome = "rejected";
          hadRejection = true;
          if (!booleanFlag(args, "json") && !booleanFlag(args, "quiet")) {
            writeHuman(`Rejected ${batch.id}: ${blocking.map((check) => check.name).join(", ")} failed; changes reverted.`);
          }
          continue;
        }
        if (inPlace) {
          // Re-verification is read-only: the checks passed, so restore the
          // user's tree instead of committing the deletion.
          restore();
        } else {
          commitAll(workingRoot, `shrike: verify ${batch.id}`);
        }
        const receipt = make({ ...batch, lines }, compared, "verified", patchPath);
        receipts.push(writeReceipt(root, receipt));
        if (!booleanFlag(args, "json") && !booleanFlag(args, "quiet")) {
          writeHuman(`Verified ${batch.id}: ${batch.symbols.length} symbols, ${batch.files.length} files.`);
        }
      }
      if (booleanFlag(args, "json")) {
        writeJson({ plan: artifact.plan, outcome, receipts });
      } else {
        writeHuman(`Shrike: re-verification ${outcome}; ${formatCount(receipts.length, "receipt")}.`);
      }
      return hadRejection ? ExitCode.VERIFICATION_FAILED : ExitCode.OK;
    } catch (error) {
      if (inPlace && !(error instanceof RevertFailedError)) {
        try {
          restoreAll(workingRoot);
        } catch {
          // Report the original failure; a failed cleanup has its own path.
        }
      }
      throw error;
    } finally {
      worktree?.cleanup();
    }
  } finally {
    pair.cleanup();
  }
}
