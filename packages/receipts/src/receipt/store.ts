import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { stableStringify } from "../canonical.ts";
import type { Receipt } from "../types.ts";
import { asValidReceipt } from "../validate.ts";

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
