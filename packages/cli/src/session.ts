import {
  booleanFlag, rejectUnknownFlags, stringFlag, type ParsedArgs,
} from "./args.ts";
import { loadConfig, engineConfig, cacheDirectory, type ShrikeConfig } from "./config.ts";
import { requestEngine } from "./engine.ts";
import { ExitCode, UsageError, CliError } from "./errors.ts";
import {
  createWorktree, headCommit, isDirty, mergeBase, resolveDefaultBase,
} from "./git.ts";
import { EMPTY_BUILD_HASH, loadBuildEvidence } from "./build.ts";
import type { ReceiptBuild } from "@shrike/receipts";
import type {
  BuildEvidenceInfo, BuildJoinResult, DeltaReport, Snapshot,
} from "./cli-types.ts";

export async function analyze(root: string, config: ShrikeConfig): Promise<Snapshot> {
  const result = await requestEngine<{ snapshot: Snapshot }>("analyze", {
    root,
    config: engineConfig(config),
    cacheDir: cacheDirectory(root, config),
  });
  return result.snapshot;
}

export interface AnalysisPair {
  base: Snapshot;
  head: Snapshot;
  delta: DeltaReport;
  baseRef: string;
  baseCommit: string;
  headCommit: string;
  dirty: boolean;
  build: BuildEvidenceInfo | null;
  cleanup: () => void;
}

export async function analyzePair(
  root: string,
  args: ParsedArgs,
  options: { allowDirty?: boolean } = {},
): Promise<AnalysisPair> {
  rejectUnknownFlags(args, [
    "json", "base", "help", "allow-dirty", "quiet", "build", "require-build-evidence",
    "run-build", "build-command", "no-client-boundary-leaks", "tier", "in-place",
    // `prune --dry-run` computes a plan without touching the tree; the flag is
    // owned by prune but must survive this shared validation.
    "dry-run",
  ]);
  const config = await loadConfig(root);
  const baseRef = resolveDefaultBase(root, stringFlag(args, "base"));
  const baseCommit = mergeBase(root, baseRef);
  const head = headCommit(root);
  const dirty = isDirty(root);
  if (dirty && !(booleanFlag(args, "allow-dirty") || options.allowDirty)) {
    throw new UsageError("working tree is dirty; commit, stash, or pass --allow-dirty");
  }
  const worktree = createWorktree(root, baseCommit, "base");
  try {
    const base = await analyze(worktree.directory, config);
    const headSnapshot = await analyze(root, config);
    const build = await attachBuildEvidence(root, headSnapshot, args, config);
    const delta = await requestEngine<DeltaReport>("delta", {
      baseSnapshot: base, headSnapshot, baseRef, baseCommit, headCommit: head, dirty,
    });
    return { base, head: headSnapshot, delta, baseRef, baseCommit, headCommit: head, dirty, build, cleanup: worktree.cleanup };
  } catch (error) {
    worktree.cleanup();
    throw error;
  }
}

export async function attachBuildEvidence(
  root: string,
  head: Snapshot,
  args: ParsedArgs,
  config: ShrikeConfig,
): Promise<BuildEvidenceInfo | null> {
  const explicit =
    stringFlag(args, "build") ?? (config.build.evidence ? config.build.snapshot : undefined);
  const required = booleanFlag(args, "require-build-evidence") || config.build.requireEvidence;
  const loaded = loadBuildEvidence(root, explicit, {
    runBuild: booleanFlag(args, "run-build"),
    buildCommand: stringFlag(args, "build-command"),
  });
  if (loaded.error) {
    throw new CliError(ExitCode.COULD_NOT_RUN, "build-evidence", loaded.error);
  }
  if (loaded.failure && required) {
    throw new CliError(
      ExitCode.COULD_NOT_RUN,
      "build-failed",
      `build evidence is required but the build failed: ${loaded.failure}`,
    );
  }
  if (booleanFlag(args, "no-client-boundary-leaks")) {
    loaded.evidence.clientBoundaryLeaks = false;
  }
  const result = await requestEngine<{ snapshot: Snapshot; join: BuildJoinResult }>("augment-build", {
    snapshot: head,
    build: loaded.evidence,
  });
  if (required && !["full", "partial"].includes(result.join.status)) {
    throw new CliError(
      ExitCode.COULD_NOT_RUN,
      "build-evidence-required",
      `build evidence is required but status is ${result.join.status}${loaded.path ? ` (${loaded.path})` : ""}${loaded.failure ? `: ${loaded.failure}` : ""}`,
    );
  }
  // Build evidence can only add liveness. The engine seeds the evidenced
  // modules and follows known edges so their dependencies are not reported dead.
  if (result.join.status === "full" || result.join.status === "partial") {
    Object.assign(head, result.snapshot);
  }
  return { evidence: loaded.evidence, join: result.join, path: loaded.path };
}

export function toReceiptBuild(info: BuildEvidenceInfo | null): ReceiptBuild {
  if (!info) {
    return {
      source: "static",
      snapshotHash: EMPTY_BUILD_HASH,
      version: "unknown",
      status: "absent",
    };
  }
  const status = info.join.status === "none" ? "absent" : info.join.status;
  return {
    source: info.evidence.source || "next analyze export",
    snapshotHash: info.evidence.snapshotHash,
    version: info.evidence.version || "unknown",
    status: status as ReceiptBuild["status"],
  };
}
