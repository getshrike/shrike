import { execFileSync, spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { booleanFlag, rejectUnknownFlags, stringFlag, type ParsedArgs } from "../args.ts";
import { loadConfig, engineConfig, type ShrikeConfig } from "../config.ts";
import { requestEngine } from "../engine.ts";
import { CliError, ExitCode, RevertFailedError, UsageError } from "../errors.ts";
import { formatCount, writeHuman, writeJson, writeStderr } from "../output.ts";
import { writeRecoverySnapshot, type RecoveryReference } from "../recovery.ts";
import { commitAll, createBranchWorktree, ensureBranch, isDirty, listRemotes, pullRequestBase, restoreAll } from "../git.ts";
import {
  classifyCheck, detectPackageManager, detectRepositoryChecks, runChecks,
  type Baseline, type CheckResult, type CheckSpec, type ComparedCheck,
} from "@shrike/verifier";
import {
  readReceipt, renderPrBody, renderReceiptComment, writeReceipt,
  type ReceiptRejection,
} from "@shrike/receipts";
import { analyzePair, toReceiptBuild, type AnalysisPair } from "../session.ts";
import { storePlan } from "../plan-store.ts";
import { loadOrCaptureBaseline } from "../baseline.ts";
import {
  bisectBatch, blockingChecks, makeReceipt, parseTier, rejectionsFor, subsetBatch,
  type BatchLike,
} from "../receipts.ts";
import { applySymbols, writePatch } from "../apply.ts";
import { renderPlan } from "../render.ts";
import type { Plan } from "../cli-types.ts";

export async function pruneCommand(root: string, args: ParsedArgs): Promise<number> {
  rejectUnknownFlags(args, [
    "json", "base", "help", "allow-dirty", "pr", "in-place", "verify", "receipt",
    "dry-run", "no-checks", "build", "require-build-evidence", "run-build",
    "build-command", "no-client-boundary-leaks", "tier",
  ]);
  if (booleanFlag(args, "no-checks")) {
    throw new UsageError("--no-checks is not supported: a verified deletion requires at least one project check");
  }
  const config = await loadConfig(root);
  const allowDirty = booleanFlag(args, "allow-dirty") || config.verification.allowDirty;
  if (isDirty(root) && !allowDirty) throw new UsageError("prune requires a clean working tree");
  const tier = parseTier(stringFlag(args, "tier"), config.verification.tier);
  const pair = await analyzePair(root, args, {
    allowDirty: config.verification.allowDirty,
    extraFlags: ["pr", "verify", "receipt"],
  });
  const plan = await requestEngine<Plan>("plan", { snapshot: pair.head, delta: pair.delta, config: engineConfig(config) });
  storePlan(root, pair, plan, tier);
  const receiptBuild = toReceiptBuild(pair.build);
  const make = (
    batch: BatchLike,
    checks: ComparedCheck[],
    outcome: string,
    patchPath: string,
    rejected: ReceiptRejection[] = [],
    recovery?: RecoveryReference,
  ) => makeReceipt(root, pair, plan, batch, checks, outcome, patchPath, rejected, recovery, receiptBuild);
  pair.cleanup();
  const dryRun = booleanFlag(args, "dry-run");
  if (plan.batches.length === 0) {
    // A dry run is the read-only equivalent of `plan`, so emit the plan
    // itself. A real prune reports its verification outcome alongside it.
    if (booleanFlag(args, "json")) writeJson(dryRun ? plan : { plan, outcome: "clean" });
    else if (dryRun) renderPlan(plan);
    else writeHuman("Shrike: no deletions to verify.");
    return ExitCode.OK;
  }
  if (dryRun) {
    if (booleanFlag(args, "json")) writeJson(plan);
    else renderPlan(plan);
    return ExitCode.FINDINGS;
  }
  const branch = `${config.pr.branchPrefix}-${plan.id.replace(/^plan_/, "")}`;
  const useWorktree = config.verification.mode === "worktree" && !booleanFlag(args, "in-place");
  if (pair.dirty && useWorktree) {
    throw new UsageError("worktree verification requires a clean tree; use --in-place with --allow-dirty");
  }
  const worktree = useWorktree ? createBranchWorktree(root, branch, pair.headCommit) : null;
  const workingRoot = worktree?.directory ?? root;
  const worktreeDirectory = worktree?.directory ?? null;
  let activeBatch: BatchLike | null = null;
  let activeCompared: ComparedCheck[] = [];
  let activePatch = "";
  let activeRecovery: RecoveryReference | null = null;
  try {
    if (!useWorktree) ensureBranch(root, branch);
    const checks = config.checks.length > 0 ? config.checks : detectRepositoryChecks(workingRoot, { tier });
    if (checks.length === 0) {
      throw new CliError(
        ExitCode.COULD_NOT_RUN,
        "no-checks",
        "no verification checks were found; configure checks or add typecheck/lint/test scripts",
      );
    }
    const baseline = await loadOrCaptureBaseline(root, workingRoot, pair.headCommit, checks, config);
    const checksEnabled = true;
    let outcome: string = "verified";
    let hadRejection = false;
    const receipts: string[] = [];

    const compare = async (): Promise<ComparedCheck[]> => {
      const after = await runChecks(checks, { cwd: workingRoot });
      return after.map((result: CheckResult) => {
        const base = baseline.checks.find((check) => check.name === result.name);
        return { ...result, baselineStatus: base?.status ?? "missing", baselineSignature: base?.signature ?? null, classification: classifyCheck(result, base) };
      });
    };
    const restore = (): void => {
      try {
        restoreAll(workingRoot);
      } catch (error) {
        throw new RevertFailedError(
          `could not restore ${workingRoot}: ${error instanceof Error ? error.message : String(error)}`,
          activeRecovery?.snapshotPath,
        );
      }
      if (process.env.SHRIKE_TEST_FAIL_REVERT === "1") {
        throw new RevertFailedError(
          "forced revert failure (SHRIKE_TEST_FAIL_REVERT=1)",
          activeRecovery?.snapshotPath,
        );
      }
      const dirty = execFileSync("git", ["status", "--porcelain"], { cwd: workingRoot, encoding: "utf8" })
        .split("\n")
        .filter((line) => line.trim().length > 0 && !line.slice(3).startsWith(".shrike/"));
      if (dirty.length > 0) {
        throw new RevertFailedError(
          `working tree is dirty after revert: ${dirty.slice(0, 5).join(", ")}`,
          activeRecovery?.snapshotPath,
        );
      }
    };
    const applyAndCompare = async (symbols: string[]): Promise<{ compared: ComparedCheck[]; blocking: ComparedCheck[]; lines: number }> => {
      restore();
      const lines = applySymbols(workingRoot, pair.head, symbols);
      const compared = checksEnabled && lines > 0 ? await compare() : [];
      return { compared, blocking: blockingChecks(compared), lines };
    };
    const snapshotBatch = (batch: BatchLike): void => {
      activeBatch = batch;
      activeRecovery = writeRecoverySnapshot(root, pair.headCommit, batch.files);
    };

    try {
      for (const batch of plan.batches) {
        snapshotBatch(batch);
        const applied = await applyAndCompare(batch.symbols);
        activeCompared = applied.compared;
        activePatch = writePatch(root, workingRoot, batch.id);
        if (applied.lines === 0) continue;
        if (applied.blocking.length === 0) {
          commitAll(workingRoot, `shrike: remove ${batch.symbols.length} dead symbols (${batch.id})`);
          const receipt = make({ ...batch, lines: applied.lines }, applied.compared, "verified", activePatch);
          receipts.push(writeReceipt(root, receipt));
          if (!booleanFlag(args, "quiet") && !booleanFlag(args, "json")) writeHuman(`Verified ${batch.id}: ${batch.symbols.length} symbols, ${batch.files.length} files.`);
          continue;
        }

        // The batch regressed. Bisect by independent units before giving up.
        restore();
        const bisection = await bisectBatch(workingRoot, pair.head, batch, checks, baseline, checksEnabled);
        if (bisection.status === "inconclusive") {
          const receipt = make({ ...batch, lines: applied.lines }, bisection.compared.length > 0 ? bisection.compared : applied.compared, "error", activePatch, rejectionsFor(batch.symbols, batch.files, applied.blocking, bisection.reason));
          receipts.push(writeReceipt(root, receipt));
          outcome = "error";
          if (booleanFlag(args, "json")) writeJson({ plan, outcome, receipts });
          else writeHuman(`Shrike: batch ${batch.id} could not be bisected (${bisection.reason}); changes reverted.`);
          return ExitCode.COULD_NOT_RUN;
        }
        if (bisection.status === "pass") {
          // The full batch passed on a rerun: the original failure was flaky.
          const retry = await applyAndCompare(batch.symbols);
          if (retry.blocking.length === 0) {
            activeCompared = retry.compared;
            const retryPatch = writePatch(root, workingRoot, batch.id);
            activePatch = retryPatch;
            commitAll(workingRoot, `shrike: remove ${batch.symbols.length} dead symbols (${batch.id})`);
            const receipt = make({ ...batch, lines: retry.lines }, retry.compared, "flaky", retryPatch);
            receipts.push(writeReceipt(root, receipt));
            if (!booleanFlag(args, "quiet") && !booleanFlag(args, "json")) writeHuman(`Verified ${batch.id} (flaky on first attempt).`);
            continue;
          }
          restore();
        }
        if (bisection.status === "interaction-failure" || bisection.rejected.length === 0 || bisection.rejected.length === batch.symbols.length) {
          const receipt = make({ ...batch, lines: applied.lines }, applied.compared, bisection.status === "interaction-failure" ? "interaction-failure" : "rejected", activePatch, rejectionsFor(batch.symbols, batch.files, applied.blocking, bisection.reason));
          receipts.push(writeReceipt(root, receipt));
          hadRejection = true;
          outcome = bisection.status === "interaction-failure" ? "interaction-failure" : "rejected";
          if (!booleanFlag(args, "json")) writeHuman(`Shrike: batch ${batch.id} rejected (${bisection.reason}); changes reverted.`);
          continue;
        }

        // Re-run the culprit alone so its rejection carries the exact check and signature.
        const culprit = bisection.rejected;
        const culpritRun = await applyAndCompare(culprit);
        activeCompared = culpritRun.compared;
        const culpritPatch = writePatch(root, workingRoot, `${batch.id}-rejected`);
        activePatch = culpritPatch;
        restore();
        const culpritBatch = subsetBatch(pair.head, batch, culprit, `${batch.id}-rejected`);
        const rejectedReceipt = make({ ...culpritBatch, lines: culpritRun.lines }, culpritRun.compared, "rejected", culpritPatch, rejectionsFor(culprit, culpritBatch.files, culpritRun.blocking, bisection.reason));
        receipts.push(writeReceipt(root, rejectedReceipt));
        hadRejection = true;
        outcome = "rejected";
        if (!booleanFlag(args, "json")) writeHuman(`Shrike: rejected ${formatCount(culprit.length, "symbol")} from ${batch.id} (${culpritRun.blocking.map((check) => check.name).join(", ")}).`);

        const remainder = batch.symbols.filter((symbol) => !culprit.includes(symbol));
        if (remainder.length === 0) continue;
        snapshotBatch(batch);
        const remainderRun = await applyAndCompare(remainder);
        activeCompared = remainderRun.compared;
        if (remainderRun.blocking.length > 0) {
          restore();
          const wholeReceipt = make({ ...batch, lines: remainderRun.lines }, remainderRun.compared, "rejected", activePatch, rejectionsFor(batch.symbols, batch.files, remainderRun.blocking, "remainder failed after bisection"));
          receipts.push(writeReceipt(root, wholeReceipt));
          continue;
        }
        const remainderBatch = subsetBatch(pair.head, batch, remainder, `${batch.id}-verified`);
        const remainderPatch = writePatch(root, workingRoot, remainderBatch.id);
        activePatch = remainderPatch;
        commitAll(workingRoot, `shrike: remove ${remainder.length} dead symbols (${batch.id})`);
        const verifiedReceipt = make({ ...remainderBatch, lines: remainderRun.lines }, remainderRun.compared, "verified", remainderPatch);
        receipts.push(writeReceipt(root, verifiedReceipt));
        if (!booleanFlag(args, "quiet") && !booleanFlag(args, "json")) writeHuman(`Verified ${remainder.length} of ${batch.symbols.length} symbols from ${batch.id}.`);
      }
    } catch (error) {
      if (!(error instanceof RevertFailedError)) throw error;
      const recovery = activeRecovery ?? {
        snapshotPath: "",
        restoreCommand: "inspect the worktree and restore it from git",
      };
      let receiptPath: string | null = null;
      if (activeBatch) {
        try {
          const receipt = make(activeBatch, activeCompared, "revert-failed", activePatch, [], recovery);
          receiptPath = writeReceipt(root, receipt);
          receipts.push(receiptPath);
        } catch (receiptError) {
          writeStderr(`Could not write revert-failed receipt: ${receiptError instanceof Error ? receiptError.message : String(receiptError)}`);
        }
      }
      const message = `revert failed; recovery snapshot: ${recovery.snapshotPath || "unavailable"}`;
      if (booleanFlag(args, "json")) {
        writeJson({ plan, outcome: "revert-failed", receipts, branch, worktree: worktreeDirectory, recovery, error: message });
      } else {
        writeStderr(`Shrike: ${message}`);
        if (recovery.snapshotPath) writeStderr(`Restore with: ${recovery.restoreCommand}`);
      }
      return ExitCode.REVERT_FAILED;
    }
    if (booleanFlag(args, "pr") && !openPullRequest(root, branch, plan, receipts, config)) {
      if (booleanFlag(args, "json")) writeJson({ plan, outcome, receipts, branch, worktree: worktreeDirectory });
      return ExitCode.COULD_NOT_RUN;
    }
    if (booleanFlag(args, "json")) writeJson({ plan, outcome, receipts, branch, worktree: worktreeDirectory });
    else {
      writeHuman(`Shrike: ${formatCount(plan.totals.symbols, "symbol")} removed on ${branch}.`);
      if (worktreeDirectory) writeHuman(`Worktree: ${worktreeDirectory}`);
      if (!booleanFlag(args, "pr")) writeHuman(`Open a draft PR with: gh pr create --draft --head ${branch}`);
    }
    return hadRejection ? ExitCode.VERIFICATION_FAILED : ExitCode.OK;
  } finally {
    worktree?.cleanup();
  }
}
export function openPullRequest(
  root: string,
  branch: string,
  plan: Plan,
  receiptPaths: string[],
  config: ShrikeConfig,
): boolean {
  const base = pullRequestBase(config.pr.base ?? plan.base.ref ?? "main", listRemotes(root));
  const receipts = receiptPaths.map((path) => readReceipt(path));
  const batches = receipts.map((receipt) => ({
    id: receipt.batch.id,
    symbols: receipt.batch.symbols.length,
    files: receipt.batch.files.length,
    lines: receipt.batch.linesRemoved,
    outcome: receipt.outcome,
    checks: receipt.checks.map((check) => `${check.name}:${check.classification ?? check.status}`).join(", "),
  }));
  const rejected = receipts.flatMap((receipt) => receipt.rejected.map((item) => ({
    symbols: item.symbols,
    check: item.check ?? "unknown",
    reason: item.reason ?? "regression",
  })));
  const body = renderPrBody({
    planId: plan.id,
    branch,
    baseRef: base,
    headCommit: plan.head.commit,
    orphanDelta: plan.totals.symbols,
    batches,
    rejected,
    receipts,
  });
  const title = config.pr.title === "Shrike: verified cleanup"
    ? `Shrike: remove ${plan.totals.symbols} dead symbols`
    : config.pr.title;
  // A remote-tracking ref can outlive a deleted branch, so ask the remote
  // instead of trusting `origin/<branch>`.
  const remote = spawnSync("git", ["ls-remote", "--heads", "origin", branch], { cwd: root, encoding: "utf8" });
  const remoteExists = remote.status === 0 && remote.stdout.trim().length > 0;
  if (remoteExists) {
    const fetched = spawnSync("git", ["fetch", "--quiet", "origin", branch], { cwd: root, encoding: "utf8" });
    if (fetched.status !== 0) {
      writeStderr(`Could not fetch origin/${branch}; refusing to force-push. Push manually after review.`);
      return false;
    }
    const ancestor = spawnSync("git", ["merge-base", "--is-ancestor", "FETCH_HEAD", "HEAD"], { cwd: root, encoding: "utf8" });
    if (ancestor.status !== 0) {
      writeStderr(`Remote branch ${branch} diverged; refusing to force-push. Push manually after review.`);
      return false;
    }
  }
  const draft = config.pr.draft ? ["--draft"] : [];
  const push = spawnSync("git", ["push", "-u", "origin", branch], { cwd: root, encoding: "utf8" });
  if (push.status !== 0) {
    const detail = (push.stderr ?? "").trim();
    writeStderr(`Could not push ${branch}${detail ? `: ${detail}` : ""}; open the PR manually: gh pr create ${draft.join(" ")} --head ${branch} --base ${base}`);
    return false;
  }
  const gh = spawnSync(
    "gh",
    ["pr", "create", ...draft, "--head", branch, "--base", base, "--title", title, "--body", body],
    { cwd: root, encoding: "utf8" },
  );
  if (gh.status !== 0) {
    const detail = (gh.stderr ?? "").trim() || (gh.error?.message ?? "");
    writeStderr(`gh pr create failed${detail ? `: ${detail}` : ""}; open the PR manually: gh pr create ${draft.join(" ")} --head ${branch} --base ${base}`);
    return false;
  }
  const url = gh.stdout.trim();
  if (config.pr.labels.length > 0) {
    for (const label of config.pr.labels) {
      const result = spawnSync("gh", ["pr", "edit", url, "--add-label", label], { cwd: root, encoding: "utf8" });
      if (result.status !== 0) writeStderr(`Could not add label ${label} to ${url}.`);
    }
  }
  writeHuman(url);
  if (!config.pr.commentReceipt) return true;
  for (const receipt of receipts) {
    const comment = spawnSync("gh", ["pr", "comment", url, "--body", renderReceiptComment(receipt)], { cwd: root, encoding: "utf8" });
    if (comment.status !== 0) writeStderr(`Could not post receipt ${receipt.id} to the PR.`);
  }
  return true;
}
