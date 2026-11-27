/**
 * Pure view model for the interactive report.
 *
 * The model is derived once from a `CleanupReport`. It never analyzes, reads
 * source files, or recomputes the dead-weight metric.
 */
import { compareText, formatCount, formatInteger } from "../report/format.ts";
import { nextCommands, sortedReportGroups } from "../report/render.ts";
import type {
  CleanupDependency,
  CleanupItem,
  CleanupReport,
  ProtectedItem,
  ReportGroup,
} from "../report/types.ts";

export interface GroupRow {
  kind: "group";
  index: number;
  directory: string;
  removableLines: number;
  removableSymbols: number;
  removableFiles: number;
  items: CleanupItem[];
}

export interface ItemRow {
  kind: "item";
  group: number;
  item: CleanupItem;
}

export interface ProtectedRow {
  kind: "protected";
  item: ProtectedItem;
}

export interface DependencyRow {
  kind: "dependency";
  item: CleanupDependency;
}

export type TuiRow = GroupRow | ItemRow | ProtectedRow | DependencyRow;

export interface TuiModel {
  report: CleanupReport;
  groups: ReportGroup[];
  rows: TuiRow[];
  protected: ProtectedItem[];
  dependencies: CleanupDependency[];
  next: string[];
}

export interface TuiSelection {
  rows: TuiRow[];
  selection: number;
}

export function buildTuiModel(report: CleanupReport): TuiModel {
  const groups = sortedReportGroups(report);
  const protectedItems = [...report.protected].sort(
    (left, right) =>
      compareText(left.path, right.path)
      || compareText(left.symbol ?? "", right.symbol ?? "")
      || compareText(left.reason, right.reason),
  );
  const dependencies = [...report.dependencies];
  const rows: TuiRow[] = [
    ...groups.flatMap((group, index): TuiRow[] => [
      {
        kind: "group",
        index,
        directory: group.directory,
        removableLines: group.removableLines,
        removableSymbols: group.removableSymbols,
        removableFiles: group.removableFiles,
        items: group.items,
      },
      ...group.items.map((item): ItemRow => ({ kind: "item", group: index, item })),
    ]),
    ...protectedItems.map((item): ProtectedRow => ({ kind: "protected", item })),
    ...dependencies.map((item): DependencyRow => ({ kind: "dependency", item })),
  ];
  return {
    report,
    groups,
    rows,
    protected: protectedItems,
    dependencies,
    next: nextCommands(report),
  };
}

export function rowPath(row: TuiRow): string {
  switch (row.kind) {
    case "group":
      return row.directory;
    case "item":
      return row.item.path;
    case "protected":
      return row.item.path;
    case "dependency":
      return `${row.item.directory}/${row.item.name}`;
  }
}

export function rowLabel(row: TuiRow): string {
  switch (row.kind) {
    case "group":
      return row.directory;
    case "item":
      return row.item.symbol ?? row.item.path;
    case "protected":
      return row.item.symbol ?? row.item.path;
    case "dependency":
      return `${row.item.package}: ${row.item.name}`;
  }
}

/** A stable, case-sensitive substring filter over the row path. */
export function filterRows(rows: readonly TuiRow[], filter: string): TuiRow[] {
  if (filter.length === 0) return [...rows];
  return rows.filter((row) => rowPath(row).includes(filter));
}

export function clampSelection(rows: readonly TuiRow[], selection: number): number {
  if (rows.length === 0) return 0;
  return Math.max(0, Math.min(rows.length - 1, Math.trunc(selection)));
}

export function selectRows(
  model: TuiModel,
  filter: string,
  selection: number,
): TuiSelection {
  const rows = filterRows(model.rows, filter);
  return { rows, selection: clampSelection(rows, selection) };
}

export function itemRows(group: GroupRow): ItemRow[] {
  return group.items.map((item) => ({ kind: "item", group: group.index, item }));
}

function nextCommand(model: TuiModel): string {
  return model.next[0] ?? "npx getshrike report";
}

export function detailLines(model: TuiModel, row: TuiRow): string[] {
  if (row.kind === "group") {
    const lines = [
      row.directory,
      `${formatCount(row.removableLines, "removable line")} · ${formatCount(row.removableSymbols, "symbol")} · ${formatCount(row.removableFiles, "file")}`,
      "",
    ];
    if (row.items.length === 0) lines.push("No removable items in this group.");
    for (const item of row.items) {
      lines.push(item.kind === "file"
        ? `${item.path}  whole file · ${formatCount(item.lines, "line")}`
        : `${item.symbol ?? item.path}  ${formatCount(item.lines, "line")} · ${item.confidence} · ${item.classification}`);
    }
    lines.push("", `Next: ${nextCommand(model)}`);
    return lines;
  }
  if (row.kind === "item") {
    const item = row.item;
    const key = item.symbol ?? item.path;
    const lines = [
      key,
      `${formatCount(item.lines, "line")} · ${item.confidence} · ${item.classification}`,
    ];
    if (item.trigger) {
      lines.push(`Trigger: ${item.trigger.change}`);
      lines.push(`File: ${item.trigger.file}`);
    }
    lines.push("");
    lines.push(item.kind === "file"
      ? "No entry and no incoming import reaches this file."
      : "No witness from any entry reaches this symbol.");
    lines.push(item.kind === "symbol"
      ? `Next: npx getshrike why ${key}`
      : `Next: ${nextCommand(model)}`);
    return lines;
  }
  if (row.kind === "protected") {
    const item = row.item;
    const lines = [
      item.symbol ?? item.path,
      `protected: ${item.reason}`,
    ];
    if (item.detail) lines.push(`Detail: ${item.detail}`);
    lines.push("This item will not be deleted.");
    return lines;
  }
  const dependency = row.item;
  return [
    `${dependency.package}: ${dependency.name}`,
    `${dependency.kind} dependency · advisory`,
    "Shrike will not remove this automatically.",
    "Review package.json before removing it.",
  ];
}

export function protectedReasonCounts(model: TuiModel): { reason: string; count: number }[] {
  const counts = new Map<string, number>();
  for (const item of model.protected) {
    counts.set(item.reason, (counts.get(item.reason) ?? 0) + 1);
  }
  return [...counts.entries()]
    .map(([reason, count]) => ({ reason, count }))
    .sort((left, right) => right.count - left.count || compareText(left.reason, right.reason));
}

export function diagnosticsSummaryLines(model: TuiModel): string[] {
  const diagnostics = model.report.diagnostics;
  if (diagnostics.total === 0) return [];
  const action = diagnostics.actionable > 0
    ? ` (${formatInteger(diagnostics.actionable)} actionable)`
    : "";
  const lines = [`Diagnostics ${formatInteger(diagnostics.total)}${action}`];
  for (const group of diagnostics.groups) {
    lines.push(`${group.category}: ${formatInteger(group.count)} × ${group.code}`);
  }
  return lines;
}
