import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { CliError, ExitCode, UsageError } from "./errors.ts";
import type { VerificationTier } from "./config.ts";
import type { Plan } from "./cli-types.ts";
import type { AnalysisPair } from "./session.ts";

export interface StoredPlan {
  schemaVersion: "1.0";
  createdAt: string;
  baseRef: string | null;
  baseCommit: string;
  headCommit: string;
  headDirty: boolean;
  tier: VerificationTier;
  plan: Plan;
}

export function planArtifactPath(root: string, id: string): string {
  return join(root, ".shrike", "plans", `${id}.json`);
}

export function storePlan(
  root: string,
  pair: AnalysisPair,
  plan: Plan,
  tier: VerificationTier,
): string {
  const artifact: StoredPlan = {
    schemaVersion: "1.0",
    createdAt: new Date().toISOString(),
    baseRef: pair.baseRef,
    baseCommit: pair.baseCommit,
    headCommit: pair.headCommit,
    headDirty: pair.dirty,
    tier,
    plan,
  };
  mkdirSync(join(root, ".shrike", "plans"), { recursive: true });
  const path = planArtifactPath(root, plan.id);
  writeFileSync(path, `${JSON.stringify(artifact, null, 2)}\n`);
  return path;
}

export function readStoredPlan(root: string, id: string): StoredPlan {
  if (!/^plan_[A-Za-z0-9]+$/.test(id)) {
    throw new UsageError("plan id must look like plan_<id>");
  }
  const path = planArtifactPath(root, id);
  if (!existsSync(path)) {
    throw new CliError(
      ExitCode.COULD_NOT_RUN,
      "plan-not-found",
      `plan ${id} was not found at .shrike/plans/${id}.json; run \`shrike plan --base <ref>\` first`,
    );
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(readFileSync(path, "utf8"));
  } catch (error) {
    throw new CliError(
      ExitCode.COULD_NOT_RUN,
      "plan-invalid",
      `plan ${id} could not be parsed: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
  const artifact = parsed as Partial<StoredPlan>;
  if (
    typeof parsed !== "object" || parsed === null ||
    artifact.schemaVersion !== "1.0" ||
    typeof artifact.baseCommit !== "string" ||
    typeof artifact.headCommit !== "string" ||
    typeof artifact.plan !== "object" || artifact.plan === null ||
    !Array.isArray(artifact.plan.batches)
  ) {
    throw new CliError(ExitCode.COULD_NOT_RUN, "plan-invalid", `plan ${id} does not match the stored-plan contract`);
  }
  return artifact as StoredPlan;
}
