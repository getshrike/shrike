import { booleanFlag, hasFlag, type ParsedArgs } from "../args.ts";
import { buildCurrentPlan } from "../current-state.ts";
import { isEligibleFile } from "../eligibility.ts";
import { UsageError } from "../errors.ts";
import { storePlan } from "../plan-store.ts";
import { branchCleanupCommand, runCleanup } from "./prune.ts";
import type { AnalysisPair } from "../session.ts";
import type { DeltaReport } from "../cli-types.ts";

/**
 * `clean` is the public name for the verified cleanup workflow.
 *
 * Without `--all` it is the same implementation as `prune`. With `--all` it
 * plans from the current snapshot and never reads a branch base.
 */
export async function cleanCommand(root: string, args: ParsedArgs): Promise<number> {
  if (!booleanFlag(args, "all")) {
    return branchCleanupCommand(root, args, "clean");
  }
  if (hasFlag(args, "base")) {
    throw new UsageError("--all and --base cannot be used together: current-state cleanup has no branch base");
  }
  return runCleanup(root, args, {
    command: "clean",
    scope: "current",
    extraFlags: ["all"],
    prepare: async ({ root: repository, args: parsed, config, tier }) => {
      const built = await buildCurrentPlan(repository, config, parsed);
      const pair: AnalysisPair = {
        base: built.snapshot,
        head: built.snapshot,
        delta: built.delta as unknown as DeltaReport,
        // The current state is its own base. A receipt still needs a name.
        baseRef: "HEAD",
        baseCommit: built.head,
        headCommit: built.head,
        dirty: built.dirty,
        build: built.build,
        cleanup: () => {},
      };
      storePlan(repository, pair, built.plan, tier);
      const wholeFiles = new Set(
        built.snapshot.unusedFiles.filter((path) => {
          const file = built.snapshot.files.find((candidate) => candidate.path === path);
          return file !== undefined && isEligibleFile(file);
        }),
      );
      return { pair, plan: built.plan, snapshot: built.snapshot, wholeFiles };
    },
  });
}
