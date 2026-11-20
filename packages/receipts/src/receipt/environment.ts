import { redactCommand, type RedactOptions } from "../redact.ts";
import type { ReceiptCheck, ReceiptEnvironment } from "../types.ts";

/** Environment block for a receipt. */
export function captureEnvironment(options: {
  packageManager?: string;
} = {}): ReceiptEnvironment {
  return {
    node: process.version,
    packageManager: options.packageManager ?? "unknown",
    platform: process.platform,
    os: process.platform,
    arch: process.arch,
    shrike: "1.0.0",
  };
}

/** Map verifier results to the smaller receipt check shape. */
export function toReceiptChecks(
  checks: Array<{
    name: string;
    command: string;
    status: string;
    baselineStatus?: string;
    durationMs: number;
    reruns: number;
    classification?: string;
    signature?: string | null;
    baselineSignature?: string | null;
    required?: boolean;
  }>,
  redact: RedactOptions = {},
): ReceiptCheck[] {
  return checks.map((check) => {
    const result: ReceiptCheck = {
      name: check.name,
      command: redactCommand(check.command, redact),
      status: check.status as ReceiptCheck["status"],
      baselineStatus: (check.baselineStatus ?? "unknown") as ReceiptCheck["baselineStatus"],
      durationMs: Math.max(0, Math.round(check.durationMs)),
      reruns: Math.max(0, Math.trunc(check.reruns)),
    };
    if (check.classification !== undefined) {
      result.classification = check.classification as ReceiptCheck["classification"];
    }
    if (check.signature) result.failureSignature = check.signature;
    if (check.required !== undefined) result.required = check.required;
    return result;
  });
}
