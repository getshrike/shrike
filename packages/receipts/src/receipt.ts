import {
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  writeFileSync,
} from "node:fs";
import { join } from "node:path";
import {
  receiptContentHash,
  receiptId,
  stableStringify,
} from "./canonical.ts";
import type {
  Receipt,
  ReceiptCheck,
  ReceiptEnvironment,
  ReceiptInput,
  ReceiptVerification,
} from "./types.ts";
import { asValidReceipt, validateReceipt } from "./validate.ts";

/** Build a receipt and derive its content hash and id. */
export function buildReceipt(input: ReceiptInput): Receipt {
  const { createdAt, ...rest } = input;
  const payload = {
    schemaVersion: "1.0" as const,
    ...rest,
    rejected: input.rejected ?? [],
  };
  const contentHash = receiptContentHash(payload);
  const receipt: Receipt = {
    ...payload,
    id: receiptId(contentHash),
    createdAt: createdAt ?? new Date().toISOString(),
    contentHash,
  };
  const validation = validateReceipt(receipt);
  if (!validation.ok) {
    throw new TypeError(`invalid receipt: ${validation.problems.join("; ")}`);
  }
  return receipt;
}

/** Recompute the content hash and id, reporting any mismatch. */
export function verifyReceipt(receipt: Receipt): ReceiptVerification {
  const problems: string[] = [];
  if (receipt.schemaVersion !== "1.0") {
    problems.push(`unsupported schemaVersion: ${String(receipt.schemaVersion)}`);
  }
  if (typeof receipt.contentHash !== "string" || receipt.contentHash.length === 0) {
    problems.push("receipt has no contentHash");
  }
  if (typeof receipt.id !== "string" || receipt.id.length === 0) {
    problems.push("receipt has no id");
  }

  const expectedHash = receiptContentHash(receipt);
  const expectedId = receiptId(expectedHash);
  if (receipt.contentHash && receipt.contentHash !== expectedHash) {
    problems.push("contentHash does not match the canonical receipt payload");
  }
  if (receipt.id && receipt.id !== expectedId) {
    problems.push("id does not match the content hash");
  }

  return {
    ok: problems.length === 0,
    expectedHash,
    expectedId,
    problems,
  };
}

/** Receipt directory under a repository root. */
export function receiptsDir(root: string): string {
  return join(root, ".shrike", "receipts");
}

/** Path for one receipt id. */
export function receiptPath(root: string, id: string): string {
  return join(receiptsDir(root), `${id}.json`);
}

/** Write a receipt to `.shrike/receipts/<id>.json`. */
export function writeReceipt(root: string, receipt: Receipt): string {
  asValidReceipt(receipt);
  const directory = receiptsDir(root);
  mkdirSync(directory, { recursive: true });
  const path = receiptPath(root, receipt.id);
  writeFileSync(path, `${stableStringify(receipt, 2)}\n`, "utf8");
  return path;
}

/** Read and validate one receipt file against the published contract. */
export function readReceipt(path: string): Receipt {
  const parsed: unknown = JSON.parse(readFileSync(path, "utf8"));
  try {
    return asValidReceipt(parsed);
  } catch (error) {
    throw new TypeError(`${error instanceof Error ? error.message : String(error)}: ${path}`);
  }
}

/** List receipt files, newest name order is lexicographic and deterministic. */
export function listReceipts(root: string): string[] {
  const directory = receiptsDir(root);
  if (!existsSync(directory)) return [];
  return readdirSync(directory)
    .filter((name) => name.startsWith("rcpt_") && name.endsWith(".json"))
    .sort();
}

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
): ReceiptCheck[] {
  return checks.map((check) => {
    const result: ReceiptCheck = {
      name: check.name,
      command: check.command,
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

/** Human-readable Markdown summary for a PR body or comment. */
export function renderReceiptMarkdown(receipt: Receipt): string {
  const lines: string[] = [];
  lines.push("## Verified cleanup");
  lines.push("");
  lines.push(
    `Batch **${receipt.batch.id}** removed **${receipt.batch.linesRemoved} lines** across ` +
      `**${receipt.batch.files.length} files** (${receipt.batch.symbols.length} symbols).`,
  );
  lines.push("");
  lines.push("| Check | Status | Baseline | Duration |");
  lines.push("|---|---|---|---|");
  for (const check of receipt.checks) {
    lines.push(
      `| ${check.name} | ${check.status} | ${check.baselineStatus ?? "missing"} | ${check.durationMs} ms |`,
    );
  }
  lines.push("");
  lines.push(`**Outcome:** ${receipt.outcome}`);
  if (receipt.rejected.length > 0) {
    lines.push("");
    lines.push("**Rejected:**");
    for (const rejection of receipt.rejected) {
      lines.push(
        `- ${rejection.symbols.join(", ") || "(batch)"} — ${rejection.reason}` +
          (rejection.check ? ` (${rejection.check})` : ""),
      );
    }
  }
  lines.push("");
  lines.push(`Receipt: \`.shrike/receipts/${receipt.id}.json\``);
  lines.push("");
  lines.push(`Content hash: \`${receipt.contentHash}\``);
  return `${lines.join("\n")}\n`;
}
