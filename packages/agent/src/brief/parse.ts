import type {
  BriefBlockedItem, BriefDeltaItem, BriefSuggestedBatch, BriefTrigger,
} from "../types.ts";
import { DEFAULT_MAX_ITEMS, isRecord, sanitizePath, sanitizeText } from "./sanitize.ts";

export function parseTrigger(value: unknown, repoRoot?: string): BriefTrigger | undefined {
  if (typeof value === "string") {
    const file = sanitizePath(value, repoRoot);
    return file.length > 0 ? { file } : undefined;
  }
  if (!isRecord(value)) return undefined;
  const file =
    typeof value.file === "string" ? sanitizePath(value.file, repoRoot) : "";
  const kind = typeof value.kind === "string" ? sanitizeText(value.kind, 64) : undefined;
  const change =
    typeof value.change === "string" ? sanitizeText(value.change, 256) : undefined;
  if (!file && !kind && !change) return undefined;
  return {
    file: file || ".",
    ...(kind ? { kind } : {}),
    ...(change ? { change } : {}),
  };
}

export function parseDeltaItem(value: unknown, repoRoot?: string): BriefDeltaItem | null {
  if (!isRecord(value)) return null;
  const symbolRaw = typeof value.symbol === "string" ? value.symbol : "";
  const fileRaw =
    typeof value.file === "string"
      ? value.file
      : symbolRaw.includes("#")
        ? symbolRaw.slice(0, symbolRaw.indexOf("#"))
        : "";
  const symbol = sanitizeText(symbolRaw, 512);
  if (!symbol) return null;
  const item: BriefDeltaItem = {
    symbol,
    file: sanitizePath(fileRaw || ".", repoRoot),
  };
  if (typeof value.kind === "string") item.kind = sanitizeText(value.kind, 64);
  if (typeof value.estimatedLines === "number" && Number.isFinite(value.estimatedLines)) {
    item.estimatedLines = Math.max(0, Math.trunc(value.estimatedLines));
  }
  const trigger = parseTrigger(value.trigger, repoRoot);
  if (trigger) item.trigger = trigger;
  return item;
}

export function parseBlockedItem(value: unknown, repoRoot?: string): BriefBlockedItem | null {
  if (!isRecord(value)) return null;
  const reason = typeof value.reason === "string" ? sanitizeText(value.reason, 256) : "";
  if (!reason) return null;
  const item: BriefBlockedItem = { reason };
  if (typeof value.symbol === "string") item.symbol = sanitizeText(value.symbol, 512);
  if (typeof value.file === "string") item.file = sanitizePath(value.file, repoRoot);
  return item;
}

export function parseSuggestedBatch(
  value: unknown,
  repoRoot?: string,
): BriefSuggestedBatch | null {
  if (!isRecord(value)) return null;
  const symbols = Array.isArray(value.symbols)
    ? value.symbols
        .filter((item): item is string => typeof item === "string")
        .slice(0, DEFAULT_MAX_ITEMS)
        .map((item) => sanitizeText(item, 512))
    : [];
  const files = Array.isArray(value.files)
    ? value.files
        .filter((item): item is string => typeof item === "string")
        .slice(0, DEFAULT_MAX_ITEMS)
        .map((item) => sanitizePath(item, repoRoot))
    : [];
  if (symbols.length === 0 && files.length === 0) return null;
  const batch: BriefSuggestedBatch = { symbols, files };
  if (typeof value.id === "string") batch.id = sanitizeText(value.id, 64);
  if (typeof value.lines === "number" && Number.isFinite(value.lines)) {
    batch.lines = Math.max(0, Math.trunc(value.lines));
  }
  return batch;
}

export function byteLength(value: unknown): number {
  return Buffer.byteLength(JSON.stringify(value), "utf8");
}
