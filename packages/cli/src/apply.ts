import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { CliError, ExitCode } from "./errors.ts";
import { resolveInside } from "./recovery.ts";
import type { Snapshot } from "./cli-types.ts";

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
    const ranges: [number, number][] = [];
    for (const key of keys) {
      const symbol = snapshot.symbols.find((candidate) => candidate.key === key && candidate.file === file);
      if (!symbol) continue;
      const startLine = symbol.span.line;
      const endLine = Math.min(lines.length, lineAt(content, symbol.span.end));
      if (!Number.isInteger(startLine) || startLine < 1 || startLine > lines.length) {
        throw new CliError(ExitCode.COULD_NOT_RUN, "invalid-span", `invalid span for ${key}`);
      }
      ranges.push([startLine - 1, Math.max(startLine - 1, endLine - 1)]);
    }
    if (ranges.length === 0) continue;
    const remove = new Set<number>();
    for (const [start, end] of ranges) {
      for (let line = start; line <= end; line += 1) remove.add(line);
      // Removing a declaration often leaves the blank line that separated it
      // from its neighbours plus the blank line on the other side. Collapse
      // that pair so the patch keeps the file's formatting.
      const before = start - 1;
      const after = end + 1;
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
    const kept = lines.filter((_line, index) => !remove.has(index));
    removed += lines.length - kept.length;
    writeFileSync(path, kept.join("\n"));
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
