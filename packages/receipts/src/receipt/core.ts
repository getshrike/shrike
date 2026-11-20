import { receiptContentHash, receiptId } from "../canonical.ts";
import type { Receipt, ReceiptInput, ReceiptVerification } from "../types.ts";
import { validateReceipt } from "../validate.ts";

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
