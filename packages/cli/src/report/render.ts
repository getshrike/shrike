/**
 * Deterministic plain-text renderer for a `CleanupReport`.
 *
 * The renderer is pure: it reads the report model, never the working tree, and
 * never recomputes the metric. Width and colour are explicit options so tests
 * do not depend on the host terminal.
 */
import {
  clampWidth,
  codePointLength,
  colorize,
  compareText,
  formatCount,
  formatInteger,
  percentage,
  renderBar,
  shortCommit,
  truncateEnd,
  truncatePath,
} from "./format.ts";
import type {
  CleanupItem,
  CleanupReport,
  ProtectedReason,
  ReportGroup,
} from "./types.ts";

export interface RenderOptions {
  width: number;
  color: boolean;
  quiet?: boolean;
  verbose?: boolean;
}

const ACCENT = 36;
const DIM = 2;

/** Resolve width in the documented order: flag, stream, env, 80. */
export function resolveWidth(
  explicit: number | undefined,
  streamWidth: number | undefined,
  envColumns: string | undefined,
): number {
  const envValue = envColumns === undefined ? undefined : Number.parseInt(envColumns, 10);
  const candidate = explicit
    ?? (Number.isFinite(streamWidth) ? streamWidth : undefined)
    ?? (Number.isFinite(envValue) ? envValue : undefined)
    ?? 80;
  return clampWidth(candidate);
}

/** Colour only when the caller allows it and the terminal is colour-capable. */
export function colorEnabled(
  streamIsTTY: boolean,
  env: Readonly<Record<string, string | undefined>>,
  json: boolean,
): boolean {
  if (json) return false;
  if (env.NO_COLOR !== undefined) return false;
  if (env.FORCE_COLOR === "0") return false;
  if (env.TERM === "dumb") return false;
  return streamIsTTY;
}

function padTo(text: string, width: number): string {
  const length = codePointLength(text);
  return length >= width ? text : `${text}${" ".repeat(width - length)}`;
}

function wrapWords(text: string, width: number): string[] {
  const words = text.split(/\s+/).filter((word) => word.length > 0);
  if (words.length === 0) return [""];
  const lines: string[] = [];
  let current = "";
  for (const word of words) {
    if (current.length === 0) {
      current = word;
    } else if (codePointLength(current) + 1 + codePointLength(word) <= width) {
      current = `${current} ${word}`;
    } else {
      lines.push(truncateEnd(current, width));
      current = word;
    }
  }
  if (current.length > 0) lines.push(truncateEnd(current, width));
  return lines;
}

function countPart(count: number, singular: string, plural = `${singular}s`): string {
  return `${formatInteger(count)} ${count === 1 ? singular : plural}`;
}

function hasRemovable(report: CleanupReport): boolean {
  return report.summary.removableFiles > 0 || report.summary.removableSymbols > 0;
}

function headerLines(report: CleanupReport, width: number): string[] {
  const narrow = width < 60;
  const scope = report.mode === "current"
    ? narrow ? "current" : "current state"
    : "branch";
  let header = `Shrike report · ${scope}`;
  if (!narrow && report.mode === "branch") {
    const ref = report.base?.ref ?? report.base?.commit;
    if (ref) header = `Shrike report · branch vs ${ref}`;
  }
  if (!narrow) header = `${header} · ${shortCommit(report.head.commit)}`;
  if (report.head.dirty) header = `${header} · dirty`;
  const lines = [truncateEnd(header, width)];
  if (report.head.dirty) {
    const warning = "Dirty tree: this report is local feedback, not a CI receipt.";
    for (const part of wrapWords(warning, width - 2)) lines.push(`  ${part}`);
  }
  return lines;
}

function barCells(width: number): number {
  if (width < 60) return 20;
  if (width < 80) return 28;
  return Math.min(48, Math.max(36, width - 44));
}

function deadWeightLines(
  report: CleanupReport,
  width: number,
  color: boolean,
): string[] {
  const summary = report.summary;
  if (summary.analyzedLines === 0) {
    return [
      "  Dead weight  n/a",
      "  No analyzable source lines were found.",
    ];
  }
  const value = percentage(summary.deadWeightBasisPoints, summary.removableLines);
  const label = colorize("Dead weight", DIM, color);
  const lines = [`  ${label}  ${colorize(value, ACCENT, color)}`];
  const fraction = `${formatCount(summary.removableLines, "removable line")} / ${formatCount(summary.analyzedLines, "analyzed source line")}`;
  for (const part of wrapWords(fraction, width - 2)) lines.push(`  ${part}`);
  const bar = renderBar(summary.deadWeightBasisPoints, barCells(width), summary.removableLines > 0);
  if (color) {
    const filled = /^█*/.exec(bar)?.[0] ?? "";
    const empty = bar.slice(filled.length);
    lines.push(`  ${colorize(filled, ACCENT, true)}${empty}`);
  } else {
    lines.push(`  ${bar}`);
  }
  return lines;
}

function countLines(report: CleanupReport, width: number): string[] {
  const summary = report.summary;
  const files = countPart(summary.removableFiles, "file");
  let symbols = countPart(summary.removableSymbols, "symbol");
  if (summary.removableExports !== summary.removableSymbols) {
    symbols = `${symbols} (${countPart(summary.removableExports, "export")})`;
  }
  const dependencies = summary.unusedDependencies > 0
    ? `${countPart(summary.unusedDependencies, "dependency", "dependencies")} (advisory)`
    : null;
  const protectedItems = summary.protectedItems > 0
    ? `${formatInteger(summary.protectedItems)} protected`
    : null;
  const diagnostics = summary.diagnostics.total > 0
    ? `${countPart(summary.diagnostics.total, "diagnostic")}${summary.diagnostics.actionable > 0
      ? ` (${formatInteger(summary.diagnostics.actionable)} actionable)`
      : ""}`
    : null;

  const first = [files, symbols, ...(dependencies ? [dependencies] : [])];
  const second = [...(protectedItems ? [protectedItems] : []), ...(diagnostics ? [diagnostics] : [])];
  const lines: string[] = [];
  if (width < 60) {
    for (const part of [...first, ...second]) lines.push(truncateEnd(`  ${part}`, width));
  } else {
    for (const parts of [first, second]) {
      if (parts.length === 0) continue;
      const joined = parts.join(" · ");
      if (codePointLength(joined) <= width - 2) {
        lines.push(`  ${joined}`);
      } else {
        for (const part of parts) lines.push(truncateEnd(`  ${part}`, width));
      }
    }
  }

  if (report.mode === "branch" && report.preexisting) {
    const { symbols: preSymbols, files: preFiles, lines: preLines } = report.preexisting;
    if (preSymbols > 0 || preFiles > 0 || preLines > 0) {
      const text = `Already dead before this branch: ${countPart(preSymbols, "symbol")}, ${countPart(preFiles, "file")}, ${countPart(preLines, "line")} (not counted)`;
      for (const part of wrapWords(text, width - 2)) lines.push(`  ${part}`);
    }
  }
  return lines;
}

function groupCounts(group: ReportGroup): string {
  return [
    countPart(group.removableLines, "line"),
    countPart(group.removableSymbols, "symbol"),
    countPart(group.removableFiles, "file"),
  ].join("   ");
}

function itemLine(item: CleanupItem): string {
  if (item.kind === "file") {
    return `    ${item.path}  (file, ${countPart(item.lines, "line")})`;
  }
  const key = item.symbol ?? `${item.path}#${item.name ?? item.id}`;
  const trigger = item.trigger ? `  [${item.trigger.kind}]` : "";
  return `    ${key}  -${countPart(item.lines, "line")}${trigger}`;
}

export function sortedReportGroups(report: CleanupReport): ReportGroup[] {
  return [...report.groups].sort(
    (left, right) =>
      right.removableLines - left.removableLines
      || compareText(left.directory, right.directory),
  );
}

function groupLines(
  report: CleanupReport,
  width: number,
  verbose: boolean,
): string[] {
  const groups = sortedReportGroups(report);
  if (groups.length === 0) return [];
  const shown = verbose ? groups : groups.slice(0, 8);
  const counts = shown.map(groupCounts);
  const longestDirectory = Math.max(...shown.map((group) => codePointLength(group.directory)));
  const widestCounts = Math.max(...counts.map((value) => codePointLength(value)));
  const oneLine = width >= 60 && width - 4 - widestCounts >= 12;
  const directoryWidth = Math.max(
    1,
    Math.min(longestDirectory, width - 4 - widestCounts),
  );
  const lines: string[] = [];
  for (const [index, group] of shown.entries()) {
    const renderedCounts = counts[index] ?? groupCounts(group);
    if (oneLine) {
      const directory = padTo(truncatePath(group.directory, directoryWidth), directoryWidth);
      lines.push(truncateEnd(`  ${directory}  ${renderedCounts}`, width));
    } else {
      lines.push(truncateEnd(`  ${truncatePath(group.directory, width - 2)}`, width));
      lines.push(truncateEnd(`    ${renderedCounts}`, width));
    }
    if (verbose) {
      const items = group.items.slice(0, 20);
      for (const item of items) lines.push(truncateEnd(itemLine(item), width));
      if (group.items.length > items.length) {
        lines.push(truncateEnd(`    ... ${formatInteger(group.items.length - items.length)} more items`, width));
      }
    }
  }
  if (!verbose && groups.length > shown.length) {
    lines.push(truncateEnd(`  ... ${formatInteger(groups.length - shown.length)} more groups (--verbose)`, width));
  }
  return lines;
}

function protectedLines(report: CleanupReport, width: number, verbose: boolean): string[] {
  if (report.summary.protectedItems === 0 || report.protected.length === 0) return [];
  const byReason = new Map<ProtectedReason, { count: number; example: string | null }>();
  for (const item of report.protected) {
    const entry = byReason.get(item.reason) ?? { count: 0, example: null };
    entry.count += 1;
    if (!entry.example) entry.example = item.symbol ?? item.path;
    byReason.set(item.reason, entry);
  }
  const rows = [...byReason.entries()].sort(
    ([leftReason, left], [rightReason, right]) =>
      right.count - left.count || compareText(leftReason, rightReason),
  );
  if (rows.length === 0) return [];
  const reasonWidth = Math.max(...rows.map(([reason]) => codePointLength(reason)));
  const lines = ["  Protected reasons"];
  for (const [reason, entry] of rows) {
    let text = `    ${padTo(reason, reasonWidth)}  ${formatInteger(entry.count)}`;
    if (verbose && entry.example) text += `  (e.g. ${entry.example})`;
    lines.push(truncateEnd(text, width));
  }
  return lines;
}

function dependencyLines(report: CleanupReport, width: number): string[] {
  if (report.dependencies.length === 0) return [];
  const lines = ["  Dependencies (advisory)"];
  for (const dependency of report.dependencies) {
    const text = `    ${dependency.package}: ${dependency.name}  [${dependency.kind}]`;
    lines.push(truncateEnd(text, width));
  }
  return lines;
}

function diagnosticsLines(report: CleanupReport, width: number, verbose: boolean): string[] {
  if (report.diagnostics.total === 0) return [];
  const action = report.diagnostics.actionable > 0
    ? ` (${formatInteger(report.diagnostics.actionable)} actionable)`
    : "";
  const lines = [`  Diagnostics  ${formatInteger(report.diagnostics.total)}${action}`];
  if (verbose) {
    for (const group of report.diagnostics.groups) {
      const files = group.files.slice(0, 3);
      const suffix = files.length > 0 ? `  [${files.join(", ")}]` : "";
      lines.push(truncateEnd(
        `    ${group.category}: ${formatInteger(group.count)} × ${group.code}${suffix}`,
        width,
      ));
    }
    return lines;
  }
  const byCategory = new Map<string, { count: number; actionable: boolean }>();
  for (const group of report.diagnostics.groups) {
    const entry = byCategory.get(group.category) ?? { count: 0, actionable: group.actionable };
    entry.count += group.count;
    entry.actionable = entry.actionable || group.actionable;
    byCategory.set(group.category, entry);
  }
  const categories = [...byCategory.entries()].sort(
    ([leftName, left], [rightName, right]) =>
      Number(right.actionable) - Number(left.actionable)
      || right.count - left.count
      || compareText(leftName, rightName),
  );
  if (categories.length > 0) {
    lines.push(truncateEnd(
      `    ${categories.map(([name, entry]) => `${name}: ${formatInteger(entry.count)}`).join(" · ")}`,
      width,
    ));
  }
  return lines;
}

export function firstWhySymbol(report: CleanupReport): string | null {
  for (const group of report.groups) {
    for (const item of group.items) {
      if (item.kind === "symbol" && item.symbol) return item.symbol;
    }
  }
  return null;
}

export function nextCommands(report: CleanupReport): string[] {
  const base = report.base?.ref ?? report.base?.commit ?? "origin/main";
  if (report.mode === "current") {
    return hasRemovable(report)
      ? ["npx getshrike clean --all", "npx getshrike report --verbose"]
      : ["npx getshrike scan", "npx getshrike install"];
  }
  if (hasRemovable(report)) {
    const commands = [`npx getshrike clean --base ${base}`];
    const symbol = firstWhySymbol(report);
    if (symbol) commands.push(`npx getshrike why ${symbol}`);
    return commands;
  }
  return [`npx getshrike delta --base ${base}`];
}

function nextLines(report: CleanupReport, width: number, empty: boolean, quiet: boolean): string[] {
  const commands = nextCommands(report);
  const lines: string[] = [];
  if (empty && !quiet) lines.push("  No removable dead weight found.");
  lines.push(truncateEnd(`  Next: ${commands[0]}`, width));
  for (const command of commands.slice(1)) {
    lines.push(truncateEnd(`        ${command}`, width));
  }
  return lines;
}

export function renderReport(report: CleanupReport, options: RenderOptions): string {
  const width = clampWidth(options.width);
  const quiet = options.quiet === true;
  const verbose = options.verbose === true;
  const sections: string[][] = [];
  sections.push(headerLines(report, width));
  sections.push(deadWeightLines(report, width, options.color));
  sections.push(countLines(report, width));
  if (!quiet) {
    sections.push(groupLines(report, width, verbose));
    sections.push(protectedLines(report, width, verbose));
    if (verbose) sections.push(dependencyLines(report, width));
    sections.push(diagnosticsLines(report, width, verbose));
  }
  const empty = !hasRemovable(report);
  sections.push(nextLines(report, width, empty, quiet));
  return sections
    .filter((section) => section.length > 0)
    .map((section) => section.join("\n"))
    .join("\n\n");
}
