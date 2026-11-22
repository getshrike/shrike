import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { CliError, ExitCode } from "./errors.ts";
import { resolveInside } from "./recovery.ts";
import type { Snapshot } from "./cli-types.ts";

type Edit = { start: number; end: number };

function lineStartsOf(content: string): number[] {
  const starts = [0];
  for (let index = 0; index < content.length; index += 1) {
    if (content[index] === "\n") starts.push(index + 1);
  }
  return starts;
}

function lineIndexAt(starts: number[], offset: number): number {
  let low = 0;
  let high = starts.length - 1;
  while (low < high) {
    const middle = Math.ceil((low + high) / 2);
    if (starts[middle]! <= offset) low = middle;
    else high = middle - 1;
  }
  return low;
}

/**
 * A named re-export specifier occupies part of one line. Deleting the whole
 * line would also remove live siblings (`export { live, dead }`), so remove
 * just the specifier and one adjacent comma. A multi-line export block puts
 * each specifier on its own line, where removing the line is cleaner.
 */
function reExportEdit(content: string, start: number, end: number): Edit {
  const starts = lineStartsOf(content);
  const index = lineIndexAt(starts, start);
  const lineStart = starts[index]!;
  const lineEnd = index + 1 < starts.length ? starts[index + 1]! : content.length;
  const before = content.slice(lineStart, start).trim();
  const after = content.slice(end, lineEnd).trim();
  const alone = before === "" && (after === "" || after === ",");
  if (alone) return { start: lineStart, end: lineEnd };
  let editStart = start;
  let editEnd = end;
  let cursor = start;
  while (cursor > lineStart && (content[cursor - 1] === " " || content[cursor - 1] === "\t")) {
    cursor -= 1;
  }
  if (cursor > 0 && content[cursor - 1] === ",") {
    editStart = cursor - 1;
  } else {
    cursor = end;
    while (cursor < content.length && (content[cursor] === " " || content[cursor] === "\t")) {
      cursor += 1;
    }
    if (content[cursor] === ",") {
      editEnd = cursor + 1;
      while (editEnd < content.length && (content[editEnd] === " " || content[editEnd] === "\t")) {
        editEnd += 1;
      }
    }
  }
  return { start: editStart, end: editEnd };
}

function mergeEdits(edits: Edit[]): Edit[] {
  const sorted = [...edits].sort((left, right) => left.start - right.start || left.end - right.end);
  const merged: Edit[] = [];
  for (const edit of sorted) {
    const previous = merged.at(-1);
    if (previous && edit.start <= previous.end) {
      previous.end = Math.max(previous.end, edit.end);
    } else {
      merged.push({ ...edit });
    }
  }
  return merged;
}

function applyEdits(content: string, edits: Edit[]): string {
  const merged = mergeEdits(edits);
  let output = "";
  let cursor = 0;
  for (const edit of merged) {
    if (edit.start < cursor) continue;
    output += content.slice(cursor, edit.start);
    cursor = edit.end;
  }
  output += content.slice(cursor);
  return output;
}

export function applySymbols(root: string, snapshot: Snapshot, symbols: string[]): number {
  const byFile = new Map<string, string[]>();
  for (const key of symbols) {
    const file = snapshot.symbols.find((candidate) => candidate.key === key)?.file;
    if (!file) continue;
    const list = byFile.get(file);
    if (list) list.push(key);
    else byFile.set(file, [key]);
  }
  // A file whose every symbol is being deleted is itself dead. Removing the
  // whole file is what prevents dangling imports from a deleted dependency
  // (for example `b.ts` importing `a.ts` when both are removed together).
  const symbolsByFile = new Map<string, string[]>();
  for (const symbol of snapshot.symbols) {
    const list = symbolsByFile.get(symbol.file);
    if (list) list.push(symbol.key);
    else symbolsByFile.set(symbol.file, [symbol.key]);
  }
  const live = new Set(snapshot.live ?? []);
  let removed = 0;
  for (const [file, keys] of [...byFile.entries()].sort(([left], [right]) => left.localeCompare(right))) {
    let path: string;
    try {
      path = resolveInside(root, file);
    } catch (error) {
      throw new CliError(
        ExitCode.COULD_NOT_RUN,
        "invalid-path",
        `refusing to edit a path outside the repository: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
    if (!existsSync(path)) continue;
    const content = readFileSync(path, "utf8");
    const lines = content.split("\n");
    const deleting = new Set(keys);
    const fileSymbols = (symbolsByFile.get(file) ?? []).filter(
      (key) => !key.endsWith("#__module__"),
    );
    if (
      fileSymbols.length > 0 &&
      fileSymbols.every((key) => deleting.has(key)) &&
      !live.has(`${file}#__module__`)
    ) {
      removed += contentLineCount(content);
      rmSync(path);
      continue;
    }
    const edits: Edit[] = [];
    const remove = new Set<number>();
    for (const key of keys) {
      const symbol = snapshot.symbols.find((candidate) => candidate.key === key && candidate.file === file);
      if (!symbol) continue;
      if (
        symbol.kind === "re-export" &&
        Number.isInteger(symbol.span.start) &&
        Number.isInteger(symbol.span.end) &&
        symbol.span.start >= 0 &&
        symbol.span.end > symbol.span.start &&
        symbol.span.end <= content.length &&
        content.slice(symbol.span.start, symbol.span.end).trim() !== ""
      ) {
        edits.push(reExportEdit(content, symbol.span.start, symbol.span.end));
        continue;
      }
      const startLine = symbol.span.line;
      const endLine = Math.min(lines.length, lineAt(content, symbol.span.end));
      if (!Number.isInteger(startLine) || startLine < 1 || startLine > lines.length) {
        throw new CliError(ExitCode.COULD_NOT_RUN, "invalid-span", `invalid span for ${key}`);
      }
      remove.add(startLine - 1);
      for (let line = startLine - 1; line <= Math.max(startLine - 1, endLine - 1); line += 1) {
        remove.add(line);
      }
      // Removing a declaration often leaves the blank line that separated it
      // from its neighbours plus the blank line on the other side. Collapse
      // that pair so the patch keeps the file's formatting.
      const before = startLine - 2;
      const after = Math.max(startLine - 1, endLine - 1) + 1;
      if (
        before >= 0 &&
        after < lines.length &&
        lines[before]?.trim() === "" &&
        lines[after]?.trim() === "" &&
        !remove.has(before) &&
        !remove.has(after)
      ) {
        remove.add(after);
      }
    }
    const starts = lineStartsOf(content);
    const sortedLines = [...remove].sort((left, right) => left - right);
    for (let index = 0; index < sortedLines.length; index += 1) {
      const first = sortedLines[index]!;
      let last = first;
      while (sortedLines[index + 1] === last + 1) {
        index += 1;
        last += 1;
      }
      const start = starts[first] ?? content.length;
      const end = starts[last + 1] ?? content.length;
      if (end > start) edits.push({ start, end });
    }
    if (edits.length === 0) continue;
    const updated = applyEdits(content, edits);
    if (updated === content) continue;
    // Line-array deletion drops the separator that followed the last kept
    // line. A byte-range edit that reaches EOF leaves it behind, so collapse
    // the extra blank line to keep the historical formatting.
    const trimmed = edits.some((edit) => edit.end === content.length) && updated.endsWith("\n\n")
      ? updated.slice(0, -1)
      : updated;
    removed += Math.max(0, contentLineCount(content) - contentLineCount(trimmed));
    writeFileSync(path, trimmed);
  }
  return removed;
}

export function contentLineCount(content: string): number {
  if (content.length === 0) return 0;
  const lines = content.split("\n");
  return content.endsWith("\n") ? lines.length - 1 : lines.length;
}

export function lineAt(content: string, offset: number): number {
  let line = 1;
  for (let index = 0; index < Math.min(offset, content.length); index += 1) if (content[index] === "\n") line += 1;
  return line;
}

export function writePatch(root: string, workingRoot: string, batchId: string): string {
  mkdirSync(join(root, ".shrike", "patches"), { recursive: true });
  const safeId = batchId.replace(/[^A-Za-z0-9_-]/g, "_");
  const relative = `.shrike/patches/${safeId}.patch`;
  writeFileSync(join(root, relative), execGitDiff(workingRoot));
  return relative;
}

export function execGitDiff(root: string): string {
  try {
    return execFileSync("git", ["diff"], { cwd: root, encoding: "utf8" });
  } catch { return ""; }
}
