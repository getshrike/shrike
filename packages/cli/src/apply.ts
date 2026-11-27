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

/**
 * Engine spans are UTF-8 byte offsets, while JavaScript string indices count
 * UTF-16 code units. Slicing a non-ASCII file with raw byte offsets cuts the
 * wrong text, so translate a byte offset before using `slice`.
 */
function indexAtByteOffset(content: string, offset: number): number {
  if (!Number.isInteger(offset) || offset <= 0) return 0;
  let bytes = 0;
  for (let index = 0; index < content.length; index += 1) {
    if (bytes >= offset) return index;
    const codePoint = content.codePointAt(index)!;
    bytes += codePoint <= 0x7f ? 1 : codePoint <= 0x7ff ? 2 : codePoint <= 0xffff ? 3 : 4;
    if (codePoint > 0xffff) index += 1;
  }
  return content.length;
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
  // The specifier is the only member of a single-line statement:
  // `export { dead } from "./impl";` or `export type { Dead } from "./impl";`.
  // Removing only the name would leave an empty `export { }` statement, which
  // still names the module after its declaration is gone.
  const onlySpecifier =
    /^export\s+(?:type\s+)?\{$/.test(before) &&
    /^\}\s*(?:from\s+["'][^"']+["'])?\s*;?\s*(?:\/\/.*)?$/.test(after);
  const alone = (before === "" && (after === "" || after === ",")) || onlySpecifier;
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

interface SpecifierRange {
  start: number;
  end: number;
}

interface NamedStatement {
  start: number;
  end: number;
  braceStart: number;
  braceEnd: number;
  specifiers: SpecifierRange[];
}

function parseSpecifierRanges(content: string, start: number, end: number): SpecifierRange[] {
  const ranges: SpecifierRange[] = [];
  let partStart = start;
  for (let index = start; index <= end; index += 1) {
    if (index !== end && content[index] !== ",") continue;
    let left = partStart;
    let right = index;
    while (left < right && /\s/.test(content[left]!)) left += 1;
    while (right > left && /\s/.test(content[right - 1]!)) right -= 1;
    // `{ type Dead }` carries the inline `type` modifier outside the name's
    // byte span. Removing only the name would leave `type` behind.
    let beforeName = left;
    while (beforeName > start && /\s/.test(content[beforeName - 1]!)) beforeName -= 1;
    let wordStart = beforeName;
    while (wordStart > start && /[A-Za-z$]/.test(content[wordStart - 1]!)) wordStart -= 1;
    if (wordStart < beforeName && content.slice(wordStart, beforeName) === "type") {
      left = wordStart;
    }
    if (left < right) ranges.push({ start: left, end: right });
    partStart = index + 1;
  }
  return ranges;
}

function namedStatements(content: string, pattern: RegExp): NamedStatement[] {
  const statements: NamedStatement[] = [];
  while (true) {
    const match = pattern.exec(content);
    if (!match) break;
    const braceStart = match.index + match[0].length - 1;
    const braceEnd = content.indexOf("}", braceStart + 1);
    if (braceEnd === -1) continue;
    let end = braceEnd + 1;
    while (end < content.length && (content[end] === " " || content[end] === "\t")) end += 1;
    if (content.startsWith("from", end)) {
      end += "from".length;
      while (end < content.length && (content[end] === " " || content[end] === "\t")) end += 1;
      const quote = content[end];
      if (quote === '"' || quote === "'") {
        end += 1;
        while (end < content.length && content[end] !== quote && content[end] !== "\n") end += 1;
        if (content[end] === quote) end += 1;
      }
    }
    while (end < content.length && (content[end] === " " || content[end] === "\t")) end += 1;
    if (content[end] === ";") end += 1;
    statements.push({
      start: match.index,
      end,
      braceStart,
      braceEnd,
      specifiers: parseSpecifierRanges(content, braceStart + 1, braceEnd),
    });
    pattern.lastIndex = end;
  }
  return statements;
}

/**
 * Locate named `export { ... }` statements. Cleanup needs this to remove a
 * whole statement when every specifier in it is dead; handling one specifier at
 * a time would leave `export { }` behind.
 */
function namedExportStatements(content: string): NamedStatement[] {
  return namedStatements(content, /\bexport\s+(?:type\s+)?\{/g);
}

/**
 * Locate named `import { ... }` statements. When a deleted symbol is only
 * imported and never referenced, the import specifier must go with it or the
 * retained file fails typecheck on a missing export. The engine records an
 * `import` edge per specifier, so the statement can be edited structurally.
 */
function namedImportStatements(content: string): NamedStatement[] {
  return namedStatements(content, /\bimport\b[^;{]*?\{/g);
}

function statementEdit(content: string, statement: NamedStatement): Edit {
  const starts = lineStartsOf(content);
  const firstLine = lineIndexAt(starts, statement.start);
  const lastLine = lineIndexAt(starts, Math.max(statement.start, statement.end - 1));
  const lineStart = starts[firstLine]!;
  const lineEnd = lastLine + 1 < starts.length ? starts[lastLine + 1]! : content.length;
  const before = content.slice(lineStart, statement.start).trim();
  const after = content.slice(statement.end, lineEnd).trim();
  if (before === "" && after === "") return { start: lineStart, end: lineEnd };
  return { start: statement.start, end: statement.end };
}

/**
 * Remove the `{ ... }` clause of an import that also has a default import.
 * `import impl, { dead } from "./impl"` must keep `impl` while dropping the
 * now-empty named clause.
 */
function namedClauseEdit(content: string, statement: NamedStatement): Edit {
  const prefix = content.slice(statement.start, statement.braceStart).trim();
  if (/^import(?:\s+type)?$/.test(prefix)) return statementEdit(content, statement);
  let start = statement.braceStart;
  let cursor = start;
  while (cursor > statement.start && /\s/.test(content[cursor - 1]!)) cursor -= 1;
  if (cursor > statement.start && content[cursor - 1] === ",") start = cursor - 1;
  return { start, end: statement.braceEnd + 1 };
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

export interface ApplyOptions {
  /** Repository-relative files to delete whole, typically `snapshot.unusedFiles`. */
  wholeFiles?: readonly string[];
}

export function applySymbols(
  root: string,
  snapshot: Snapshot,
  symbols: string[],
  options: ApplyOptions = {},
): number {
  const byKey = new Map(snapshot.symbols.map((symbol) => [symbol.key, symbol]));
  const wholeFiles = new Set<string>(options.wholeFiles ?? []);
  for (const key of symbols) {
    if (!key.endsWith("#__module__")) continue;
    const symbol = byKey.get(key);
    if (symbol) wholeFiles.add(symbol.file);
  }
  const byFile = new Map<string, string[]>();
  for (const key of symbols) {
    if (key.endsWith("#__module__")) continue;
    const symbol = byKey.get(key);
    if (!symbol) continue;
    const list = byFile.get(symbol.file);
    if (list) list.push(key);
    else byFile.set(symbol.file, [key]);
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
  const fileByPath = new Map((snapshot.files ?? []).map((file) => [file.path, file]));
  const deletedKeys = new Set(symbols);
  const localReExports = new Map<string, NonNullable<Snapshot["edges"]>[number][]>();
  const deletedImports = new Map<string, NonNullable<Snapshot["edges"]>[number][]>();
  for (const edge of snapshot.edges ?? []) {
    if (edge.kind === "re-export" && edge.source === edge.target && deletedKeys.has(edge.source)) {
      const list = localReExports.get(edge.file);
      if (list) list.push(edge);
      else localReExports.set(edge.file, [edge]);
    }
    if (
      edge.kind === "import" &&
      deletedKeys.has(edge.target) &&
      !edge.target.endsWith("#__module__") &&
      Number.isInteger(edge.span.start) &&
      Number.isInteger(edge.span.end) &&
      edge.span.start >= 0 &&
      edge.span.end > edge.span.start
    ) {
      const list = deletedImports.get(edge.file);
      if (list) list.push(edge);
      else deletedImports.set(edge.file, [edge]);
    }
  }
  let removed = 0;
  for (const file of [...wholeFiles].sort((left, right) => left.localeCompare(right))) {
    const record = fileByPath.get(file);
    if (!record) continue;
    if (record.publicApi || record.generated || record.parseStatus !== "ok") continue;
    if (live.has(`${file}#__module__`)) continue;
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
    removed += record.lines > 0 ? record.lines : contentLineCount(content);
    rmSync(path);
  }
  const editFiles = [...new Set([...byFile.keys(), ...deletedImports.keys()])]
    .sort((left, right) => left.localeCompare(right));
  for (const file of editFiles) {
    const keys = byFile.get(file) ?? [];
    if (wholeFiles.has(file)) continue;
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
    // Collect every specifier this batch deletes in this file, including local
    // `export { Name }` edges. If all specifiers of a statement are here, the
    // whole statement is removed below instead of leaving `export { }`.
    const deletedSpecifierSpans: SpecifierRange[] = [];
    for (const key of keys) {
      const symbol = snapshot.symbols.find(
        (candidate) => candidate.key === key && candidate.file === file,
      );
      if (
        !symbol ||
        symbol.kind !== "re-export" ||
        !Number.isInteger(symbol.span.start) ||
        !Number.isInteger(symbol.span.end) ||
        symbol.span.start < 0 ||
        symbol.span.end <= symbol.span.start
      ) {
        continue;
      }
      const start = indexAtByteOffset(content, symbol.span.start);
      const end = indexAtByteOffset(content, symbol.span.end);
      if (end > start && content.slice(start, end).trim() !== "") {
        deletedSpecifierSpans.push({ start, end });
      }
    }
    for (const edge of localReExports.get(file) ?? []) {
      if (
        !Number.isInteger(edge.span.start) ||
        !Number.isInteger(edge.span.end) ||
        edge.span.end <= edge.span.start
      ) {
        continue;
      }
      const start = indexAtByteOffset(content, edge.span.start);
      const end = indexAtByteOffset(content, edge.span.end);
      if (end > start && content.slice(start, end).trim() !== "") {
        deletedSpecifierSpans.push({ start, end });
      }
    }
    for (const statement of namedExportStatements(content)) {
      if (statement.specifiers.length === 0) continue;
      const fullyDeleted = statement.specifiers.every((specifier) =>
        deletedSpecifierSpans.some(
          (span) => span.start < specifier.end && specifier.start < span.end,
        ),
      );
      if (fullyDeleted) edits.push(statementEdit(content, statement));
    }
    // A local `export { name }` clause names the declaration but is not part of
    // its span. The engine records a same-file, same-symbol re-export edge for
    // that specifier; remove it with the declaration or the export is left
    // naming a missing binding.
    for (const edge of localReExports.get(file) ?? []) {
      if (
        !Number.isInteger(edge.span.start) ||
        !Number.isInteger(edge.span.end) ||
        edge.span.end <= edge.span.start
      ) {
        continue;
      }
      const start = indexAtByteOffset(content, edge.span.start);
      const end = indexAtByteOffset(content, edge.span.end);
      if (end <= start || content.slice(start, end).trim() === "") continue;
      edits.push(reExportEdit(content, start, end));
    }
    for (const key of keys) {
      const symbol = snapshot.symbols.find((candidate) => candidate.key === key && candidate.file === file);
      if (!symbol) continue;
      if (
        symbol.kind === "re-export" &&
        Number.isInteger(symbol.span.start) &&
        Number.isInteger(symbol.span.end) &&
        symbol.span.start >= 0 &&
        symbol.span.end > symbol.span.start
      ) {
        const start = indexAtByteOffset(content, symbol.span.start);
        const end = indexAtByteOffset(content, symbol.span.end);
        if (end > start && content.slice(start, end).trim() !== "") {
          edits.push(reExportEdit(content, start, end));
          continue;
        }
      }
      const startLine = symbol.span.line;
      // `span.end` is an exclusive byte offset and can point past the blank
      // line that follows a declaration. Counting those newlines would delete
      // the next declaration, so prefer the engine's inclusive `endLine` and
      // keep the byte-offset fallback for older or synthetic spans.
      const reportedEnd = symbol.span.endLine;
      const endLine = typeof reportedEnd === "number"
        && Number.isInteger(reportedEnd)
        && reportedEnd >= startLine
        ? Math.min(lines.length, reportedEnd)
        : Math.min(lines.length, lineAt(content, indexAtByteOffset(content, symbol.span.end)));
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
    // A retained file can import a symbol that this batch deletes. The import
    // is then unused by any live code, but TypeScript still validates that the
    // named export exists, so remove the specifier or the whole statement.
    const importSpans: SpecifierRange[] = [];
    for (const edge of deletedImports.get(file) ?? []) {
      const start = indexAtByteOffset(content, edge.span.start);
      const end = indexAtByteOffset(content, edge.span.end);
      if (end > start && content.slice(start, end).trim() !== "") {
        importSpans.push({ start, end });
      }
    }
    for (const statement of namedImportStatements(content)) {
      if (statement.specifiers.length === 0) continue;
      const deleted = statement.specifiers.filter((specifier) =>
        importSpans.some(
          (span) => span.start < specifier.end && specifier.start < span.end,
        ),
      );
      if (deleted.length === 0) continue;
      if (deleted.length === statement.specifiers.length) {
        edits.push(namedClauseEdit(content, statement));
      } else {
        for (const specifier of deleted) {
          edits.push(reExportEdit(content, specifier.start, specifier.end));
        }
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
