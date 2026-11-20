import type { BriefBlockedItem, BriefDeltaItem, CleanupBrief, CleanupBriefInput } from "../types.ts";
import {
  DEFAULT_MAX_BYTES, DEFAULT_MAX_ITEMS, UNTRUSTED_NOTICE, isRecord, sanitizePath, sanitizeText,
} from "./sanitize.ts";
import {
  byteLength, parseBlockedItem, parseDeltaItem, parseSuggestedBatch,
} from "./parse.ts";

/**
 * Build a bounded CleanupBrief from an engine delta.
 *
 * Caps at 20 items and 32 KB by default, strips control characters, and marks
 * the repository content untrusted. The brief is advisory only.
 */
export function buildCleanupBrief(input: CleanupBriefInput): CleanupBrief {
  const maxItems = Math.max(1, input.maxItems ?? DEFAULT_MAX_ITEMS);
  const maxBytes = Math.max(1_024, input.maxBytes ?? DEFAULT_MAX_BYTES);
  const delta = isRecord(input.delta) ? input.delta : {};

  const rawItems = Array.isArray(delta.newlyDead) ? delta.newlyDead : [];
  const allItems: BriefDeltaItem[] = [];
  for (const raw of rawItems) {
    const item = parseDeltaItem(raw, input.repo);
    if (item) allItems.push(item);
  }

  const rawBlocked = Array.isArray(input.blocked)
    ? input.blocked
    : Array.isArray(delta.blocked)
      ? delta.blocked
      : [];
  const blocked: BriefBlockedItem[] = [];
  for (const raw of rawBlocked.slice(0, maxItems)) {
    const item = parseBlockedItem(raw, input.repo);
    if (item) blocked.push(item);
  }

  const filesFromDelta = Array.isArray(delta.newlyDeadFiles)
    ? delta.newlyDeadFiles.filter((item): item is string => typeof item === "string")
    : [];
  const itemFiles = [...new Set(allItems.map((item) => item.file))];
  const files = filesFromDelta.length > 0
    ? [...new Set(filesFromDelta.map((file) => sanitizePath(file, input.repo)))]
    : itemFiles;

  const changed = new Set(
    (input.changedFiles ?? []).map((file) => sanitizePath(file, input.repo)),
  );
  const untouched = files.filter((file) => !changed.has(file)).length;

  const cascade = isRecord(delta.cascade) ? delta.cascade : {};
  const cascadeLines =
    typeof cascade.lines === "number" && Number.isFinite(cascade.lines)
      ? Math.max(0, Math.trunc(cascade.lines))
      : undefined;
  const estimatedLines =
    cascadeLines ??
    allItems.reduce((sum, item) => sum + (item.estimatedLines ?? 0), 0);

  const base = sanitizeText(input.base, 256) || "origin/main";
  const brief: CleanupBrief = {
    schemaVersion: "1.0",
    kind: "cleanup-brief",
    notice: UNTRUSTED_NOTICE,
    repo: sanitizeText(input.repo, 256),
    base,
    head: sanitizeText(input.head, 128) || "HEAD",
    dirty: Boolean(input.dirty),
    newlyDead: allItems.length,
    files: files.length,
    untouchedFiles: untouched,
    estimatedLines,
    checks: sanitizeText(input.checks ?? "skipped", 64),
    preexisting: Math.max(0, Math.trunc(input.preexisting ?? 0)),
    top: allItems.slice(0, maxItems),
    blocked: blocked.slice(0, maxItems),
    suggestedBatch: parseSuggestedBatch(input.suggestedBatch, input.repo),
    commands: {
      plan: `shrike plan --base ${base}`,
      prune: `shrike prune --base ${base} --verify --receipt`,
      proof: `shrike proof ${allItems[0]?.symbol ?? "<symbol>"}`,
    },
    limits: { maxItems, maxBytes },
    nextCommand: `shrike plan --base ${base}`,
    truncated: allItems.length > maxItems,
  };

  // Enforce the byte cap by trimming the advisory payload, never the notice.
  while (byteLength(brief) > maxBytes && brief.top.length > 0) {
    brief.top.pop();
    brief.truncated = true;
  }
  while (byteLength(brief) > maxBytes && brief.blocked.length > 0) {
    brief.blocked.pop();
    brief.truncated = true;
  }
  if (byteLength(brief) > maxBytes) {
    brief.suggestedBatch = null;
    brief.truncated = true;
  }
  if (byteLength(brief) > maxBytes) {
    brief.repo = sanitizeText(brief.repo, 64);
    brief.notice = UNTRUSTED_NOTICE.slice(0, 128);
    brief.truncated = true;
  }

  return brief;
}
