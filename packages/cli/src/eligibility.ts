import type { FileRecord } from "./cli-types.ts";

/** Source module kinds the cleanup plan may remove whole. */
export const ELIGIBLE_MODULE_KINDS: ReadonlySet<string> = new Set([
  "ts",
  "tsx",
  "js",
  "jsx",
  "mjs",
  "cjs",
  "dts",
]);

/**
 * A file is removable only when the analyzer parsed it, it is not generated,
 * and it is source the cleanup layer understands. Everything else stays.
 */
export function isEligibleFile(file: Pick<FileRecord, "parseStatus" | "generated" | "moduleKind">): boolean {
  return file.parseStatus === "ok"
    && !file.generated
    && ELIGIBLE_MODULE_KINDS.has(file.moduleKind);
}
