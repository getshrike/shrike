import { runChecks } from "./run.ts";
import type { Baseline, CheckSpec, RunOptions } from "./types.ts";

export interface CaptureBaselineOptions {
  head: string;
  specs: CheckSpec[];
  cwd?: string;
  env?: NodeJS.ProcessEnv;
  configHash?: string;
  /** Injectable clock for deterministic tests. */
  createdAt?: string;
}

/**
 * Capture the baseline before a deletion.
 *
 * The baseline is valid for one head commit and one check configuration. The
 * caller is responsible for storing it with the head and config hash.
 */
export async function captureBaseline(
  options: CaptureBaselineOptions,
): Promise<Baseline> {
  const runOptions: RunOptions = {
    cwd: options.cwd,
    env: options.env,
  };
  const checks = await runChecks(options.specs, runOptions);
  return {
    schemaVersion: "1.0",
    head: options.head,
    capturedAt: options.createdAt ?? new Date().toISOString(),
    checks,
    ...(options.configHash ? { configHash: options.configHash } : {}),
  };
}

/** An empty baseline, used when the caller has no checks to run. */
export function emptyBaseline(
  head: string,
  options: { configHash?: string; createdAt?: string } = {},
): Baseline {
  return {
    schemaVersion: "1.0",
    head,
    capturedAt: options.createdAt ?? new Date().toISOString(),
    checks: [],
    ...(options.configHash ? { configHash: options.configHash } : {}),
  };
}
