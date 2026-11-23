import { isAbsolute, join } from "node:path";

import { capOutput, failureSignature, normalizeOutput } from "../normalize.ts";
import type { CheckResult, CheckSpec, CheckStatus, RunOptions } from "../types.ts";
import { combinedOutput, executeCommand } from "./execute.ts";

/** Run one check once. Reruns are handled by `runCheckWithReruns`. */
export async function runCheck(
  spec: CheckSpec,
  options: RunOptions = {},
): Promise<CheckResult> {
  const cwd = spec.cwd === undefined
    ? options.cwd
    : isAbsolute(spec.cwd)
      ? spec.cwd
      : join(options.cwd ?? process.cwd(), spec.cwd);
  const raw = await executeCommand(spec.command, {
    ...options,
    cwd,
    timeoutMs: spec.timeoutMs ?? options.timeoutMs,
    env: { ...options.env, ...spec.env },
  });

  let status: CheckStatus;
  if (raw.spawnError) {
    status = "error";
  } else if (raw.timedOut) {
    status = "timeout";
  } else if (raw.exitCode === 0) {
    status = "pass";
  } else {
    status = "fail";
  }

  const combined = combinedOutput(raw);
  const normalized = normalizeOutput(combined, {
    cwd,
    root: options.cwd,
  });
  const signature =
    status === "pass" ? null : failureSignature(combined, { cwd, root: options.cwd });

  return {
    name: spec.name,
    command: spec.command,
    status,
    exitCode: raw.exitCode,
    durationMs: raw.durationMs,
    output: capOutput(normalized),
    signature,
    timedOut: raw.timedOut,
    reruns: 0,
    flaky: false,
    required: spec.required ?? true,
  };
}

/**
 * Run a check with optional reruns.
 *
 * A failure followed by a pass is `flaky` (never a clean pass). A set of
 * failures with different signatures is also marked flaky, per
 * docs/architecture.md.
 */
export async function runCheckWithReruns(
  spec: CheckSpec,
  options: RunOptions = {},
): Promise<CheckResult> {
  const first = await runCheck(spec, options);
  if (first.status === "pass" || first.status === "skipped") return first;

  const extraRuns = Math.max(0, spec.reruns ?? 0);
  if (extraRuns === 0) return first;

  const failures: CheckResult[] = [first];
  let totalDuration = first.durationMs;

  for (let attempt = 1; attempt <= extraRuns; attempt += 1) {
    const next = await runCheck(spec, options);
    totalDuration += next.durationMs;
    if (next.status === "pass") {
      return {
        ...next,
        durationMs: totalDuration,
        reruns: attempt,
        flaky: true,
        signature: first.signature,
        output: first.output,
      };
    }
    failures.push(next);
  }

  const signatures = new Set(
    failures.map((result) => result.signature).filter((value): value is string => Boolean(value)),
  );
  const last = failures[failures.length - 1] ?? first;
  return {
    ...last,
    durationMs: totalDuration,
    reruns: extraRuns,
    flaky: signatures.size > 1,
    signature: first.signature,
    output: first.output,
  };
}

/** Run checks sequentially so results are deterministic and resource-bounded. */
export async function runChecks(
  specs: CheckSpec[],
  options: RunOptions = {},
): Promise<CheckResult[]> {
  const results: CheckResult[] = [];
  for (const spec of specs) {
    results.push(await runCheckWithReruns(spec, options));
  }
  return results;
}
