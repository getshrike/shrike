/**
 * The single dead-weight metric.
 *
 * `removableLines` is the union of removable symbol spans plus whole removable
 * files. `analyzedLines` is the sum of eligible source files. The function only
 * reads fields already present in the snapshot; it never touches the working
 * tree.
 */
import { CliError, ExitCode } from "../errors.ts";
import type { FileRecord, Snapshot, SymbolRecord } from "../cli-types.ts";
import { isEligibleFile } from "../eligibility.ts";

export { isEligibleFile } from "../eligibility.ts";

export interface MetricResult {
  analyzedFiles: number;
  analyzedLines: number;
  removableFiles: number;
  removableSymbols: number;
  removableExports: number;
  removableLines: number;
  deadWeightBasisPoints: number;
  uncountedRemovableSymbols: number;
}

export interface Interval {
  start: number;
  end: number;
}

export interface RemovableFile {
  file: FileRecord;
  wholeFile: boolean;
  intervals: Interval[];
  lines: number;
  symbols: SymbolRecord[];
}

export function mergeIntervals(intervals: readonly Interval[]): Interval[] {
  const sorted = [...intervals]
    .filter((interval) => Number.isInteger(interval.start) && Number.isInteger(interval.end))
    .filter((interval) => interval.end >= interval.start)
    .sort((left, right) => left.start - right.start || left.end - right.end);
  const merged: Interval[] = [];
  for (const interval of sorted) {
    const previous = merged.at(-1);
    if (previous && interval.start <= previous.end + 1) {
      previous.end = Math.max(previous.end, interval.end);
    } else {
      merged.push({ ...interval });
    }
  }
  return merged;
}

export function intervalLength(intervals: readonly Interval[]): number {
  return mergeIntervals(intervals).reduce(
    (total, interval) => total + interval.end - interval.start + 1,
    0,
  );
}

function symbolIntervals(file: FileRecord, symbols: readonly SymbolRecord[]): Interval[] {
  const intervals: Interval[] = [];
  for (const symbol of symbols) {
    const start = Math.max(1, symbol.span.line);
    const end = Math.min(file.lines, symbol.span.line + Math.max(0, symbol.lines - 1));
    if (end >= start) intervals.push({ start, end });
  }
  return mergeIntervals(intervals);
}

/**
 * Compute the removable line intervals for every eligible file that contains a
 * removable item. One pass over the snapshot keeps the builder linear in the
 * size of the report.
 */
export function removableFileLines(
  snapshot: Snapshot,
  symbolKeys: ReadonlySet<string>,
  wholeFiles: ReadonlySet<string>,
): Map<string, RemovableFile> {
  const symbolsByFile = new Map<string, SymbolRecord[]>();
  for (const symbol of snapshot.symbols) {
    if (!symbolKeys.has(symbol.key) || symbol.key.endsWith("#__module__")) continue;
    const list = symbolsByFile.get(symbol.file);
    if (list) list.push(symbol);
    else symbolsByFile.set(symbol.file, [symbol]);
  }

  const result = new Map<string, RemovableFile>();
  for (const file of snapshot.files) {
    if (!isEligibleFile(file)) continue;
    const wholeFile = wholeFiles.has(file.path);
    const symbols = symbolsByFile.get(file.path) ?? [];
    if (!wholeFile && symbols.length === 0) continue;
    const intervals = wholeFile
      ? file.lines > 0 ? [{ start: 1, end: file.lines }] : []
      : symbolIntervals(file, symbols);
    result.set(file.path, {
      file,
      wholeFile,
      intervals,
      lines: intervals.reduce((total, interval) => total + interval.end - interval.start + 1, 0),
      symbols,
    });
  }
  return result;
}

export function removableLinesFor(
  snapshot: Snapshot,
  symbolKeys: ReadonlySet<string>,
  wholeFiles: ReadonlySet<string>,
): number {
  let total = 0;
  for (const entry of removableFileLines(snapshot, symbolKeys, wholeFiles).values()) {
    total += entry.lines;
  }
  return total;
}

export function computeMetric(
  snapshot: Snapshot,
  symbolKeys: ReadonlySet<string>,
  wholeFiles: ReadonlySet<string>,
  removableFiles?: ReadonlyMap<string, RemovableFile>,
  byKey?: ReadonlyMap<string, SymbolRecord>,
): MetricResult {
  let analyzedFiles = 0;
  let analyzedLines = 0;
  for (const file of snapshot.files) {
    if (!isEligibleFile(file)) continue;
    analyzedFiles += 1;
    analyzedLines += file.lines;
  }

  const files = removableFiles ?? removableFileLines(snapshot, symbolKeys, wholeFiles);
  let removableLines = 0;
  for (const entry of files.values()) removableLines += entry.lines;

  let removableSymbols = 0;
  let removableExports = 0;
  let uncountedRemovableSymbols = 0;
  const symbols = byKey ?? new Map(snapshot.symbols.map((symbol) => [symbol.key, symbol]));
  for (const key of symbolKeys) {
    if (key.endsWith("#__module__")) continue;
    removableSymbols += 1;
    const symbol = symbols.get(key);
    if (!symbol) {
      uncountedRemovableSymbols += 1;
      continue;
    }
    if (symbol.exported) removableExports += 1;
    if (!files.has(symbol.file)) uncountedRemovableSymbols += 1;
  }

  if (removableLines > analyzedLines) {
    throw new CliError(
      ExitCode.COULD_NOT_RUN,
      "report-metric",
      `the removable line count (${removableLines}) exceeds the analyzed line count (${analyzedLines}); refusing to report an impossible metric`,
    );
  }
  const deadWeightBasisPoints = analyzedLines === 0
    ? 0
    : Math.round((removableLines * 10000) / analyzedLines);
  return {
    analyzedFiles,
    analyzedLines,
    removableFiles: files.size,
    removableSymbols,
    removableExports,
    removableLines,
    deadWeightBasisPoints,
    uncountedRemovableSymbols,
  };
}
