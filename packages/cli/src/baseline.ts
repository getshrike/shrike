import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";

import { captureBaseline, type Baseline, type CheckSpec } from "@shrike/verifier";
import type { ShrikeConfig } from "./config.ts";
import { ConfigError } from "./errors.ts";
import { resolveInside } from "./recovery.ts";

export function baselineConfigHash(checks: CheckSpec[]): string {
  const normalized = checks.map((check) => ({
    name: check.name,
    command: check.command,
    cwd: check.cwd ?? null,
    timeoutMs: check.timeoutMs ?? null,
    reruns: check.reruns ?? null,
    required: check.required ?? true,
  }));
  return `sha256:${createHash("sha256").update(JSON.stringify(normalized)).digest("hex")}`;
}

/**
 * Reuse a baseline only when it belongs to this head commit and this exact
 * check set. Anything else is discarded, never compared against.
 */
export async function loadOrCaptureBaseline(
  root: string,
  workingRoot: string,
  head: string,
  checks: CheckSpec[],
  config: ShrikeConfig,
): Promise<Baseline> {
  let path: string;
  try {
    path = resolveInside(root, config.verification.baselinePath);
  } catch (error) {
    throw new ConfigError(
      `verification.baselinePath must stay inside the repository: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
  const configHash = baselineConfigHash(checks);
  if (existsSync(path)) {
    try {
      const parsed = JSON.parse(readFileSync(path, "utf8")) as Baseline;
      if (
        parsed.schemaVersion === "1.0" &&
        parsed.head === head &&
        parsed.configHash === configHash &&
        Array.isArray(parsed.checks)
      ) {
        return parsed;
      }
    } catch {
      // A corrupt baseline is discarded and rebuilt.
    }
  }
  const baseline = await captureBaseline({
    head,
    specs: checks,
    cwd: workingRoot,
    configHash,
  });
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, `${JSON.stringify(baseline, null, 2)}\n`);
  return baseline;
}
