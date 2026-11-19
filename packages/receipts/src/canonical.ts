import { createHash } from "node:crypto";
import type { Receipt } from "./types.ts";

/**
 * Canonical JSON and content hashing.
 *
 * Object keys are sorted recursively; arrays keep their order. The hash payload
 * excludes `createdAt`, `id`, and `contentHash` so the id can be derived from
 * the hash without circularity.
 */

function normalizeValue(value: unknown): unknown {
  if (value === null) return null;
  if (typeof value === "string" || typeof value === "boolean") return value;
  if (typeof value === "number") {
    if (!Number.isFinite(value)) {
      throw new TypeError("canonical JSON cannot encode a non-finite number");
    }
    return value;
  }
  if (Array.isArray(value)) {
    return value.map((item) => (item === undefined ? null : normalizeValue(item)));
  }
  if (typeof value === "object") {
    const record = value as Record<string, unknown>;
    const result: Record<string, unknown> = {};
    for (const key of Object.keys(record).sort()) {
      const item = record[key];
      if (item === undefined) continue;
      result[key] = normalizeValue(item);
    }
    return result;
  }
  throw new TypeError(`canonical JSON cannot encode ${typeof value}`);
}

/** Recursively sort object keys and drop undefined object properties. */
export function canonicalize(value: unknown): unknown {
  return normalizeValue(value);
}

/** Canonical JSON with sorted keys and no insignificant whitespace. */
export function canonicalJson(value: unknown): string {
  return JSON.stringify(canonicalize(value));
}

/** Canonical pretty JSON, useful for deterministic receipt files. */
export function stableStringify(value: unknown, space = 0): string {
  return JSON.stringify(canonicalize(value), null, space);
}

/** SHA-256 of canonical JSON as `sha256:<hex>`. */
export function hashCanonical(value: unknown): string {
  const digest = createHash("sha256")
    .update(canonicalJson(value), "utf8")
    .digest("hex");
  return `sha256:${digest}`;
}

type HashableReceipt = Omit<Receipt, "id" | "createdAt" | "contentHash">;

/** The exact object hashed to derive a receipt id and content hash. */
export function receiptHashPayload(
  receipt: HashableReceipt | Receipt,
): HashableReceipt {
  const {
    id: _id,
    createdAt: _createdAt,
    contentHash: _contentHash,
    ...payload
  } = receipt as Receipt;
  return payload;
}

/** Content hash of a receipt, excluding createdAt, id, and contentHash. */
export function receiptContentHash(receipt: HashableReceipt | Receipt): string {
  return hashCanonical(receiptHashPayload(receipt));
}

/** Receipt id derived from the first 24 hex characters of the content hash. */
export function receiptId(contentHash: string): string {
  const hex = contentHash.replace(/^sha256:/, "");
  return `rcpt_${hex.slice(0, 24)}`;
}
