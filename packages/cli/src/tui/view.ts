/**
 * Read-only Ink view for a `CleanupReport`.
 *
 * Kept in a `.ts` module with `createElement` calls so the view model and
 * rendering tests can run without a JSX transform. `app.tsx` is the public
 * component entry and `main.tsx` wires it to Ink.
 */
import { Box, Text, useApp, useInput, useStdout } from "ink";
import { createElement as h, useState } from "react";

import {
  formatCount,
  formatInteger,
  percentage,
  renderBar,
  shortCommit,
  truncateEnd,
  truncatePath,
} from "../report/format.ts";
import type { CleanupReport } from "../report/types.ts";
import {
  buildTuiModel,
  clampSelection,
  detailLines,
  diagnosticsSummaryLines,
  protectedReasonCounts,
  rowLabel,
  selectRows,
  type TuiModel,
  type TuiRow,
} from "./model.ts";

export interface ReportAppProps {
  report: CleanupReport;
  width?: number;
  height?: number;
}

function scopeLabel(model: TuiModel, width: number): string {
  const report = model.report;
  if (report.mode === "current") return width < 60 ? "current" : "current state";
  return "branch";
}

function headerLine(model: TuiModel, width: number): string {
  const report = model.report;
  let header = `Shrike · ${scopeLabel(model, width)}`;
  if (report.mode === "branch" && width >= 60) {
    const ref = report.base?.ref ?? report.base?.commit;
    if (ref) header = `Shrike · branch vs ${ref}`;
  }
  if (width >= 60) header = `${header} · ${shortCommit(report.head.commit)}`;
  if (report.head.dirty) header = `${header} · dirty`;
  return truncateEnd(header, width);
}

function barCells(width: number): number {
  if (width < 60) return 20;
  if (width < 80) return 28;
  return Math.min(48, Math.max(36, width - 44));
}

function deadWeightLines(model: TuiModel, width: number): string[] {
  const summary = model.report.summary;
  if (summary.analyzedLines === 0) {
    return ["Dead weight  n/a", "No analyzable source lines were found."];
  }
  return [
    `Dead weight  ${percentage(summary.deadWeightBasisPoints, summary.removableLines)}`,
    truncateEnd(
      `${formatCount(summary.removableLines, "removable line")} / ${formatCount(summary.analyzedLines, "analyzed source line")}`,
      width,
    ),
    renderBar(summary.deadWeightBasisPoints, barCells(width), summary.removableLines > 0),
  ];
}

function countLines(model: TuiModel, width: number): string[] {
  const summary = model.report.summary;
  const first = [
    formatCount(summary.removableFiles, "file"),
    formatCount(summary.removableSymbols, "symbol"),
    ...(summary.unusedDependencies > 0
      ? [`${formatCount(summary.unusedDependencies, "dependency", "dependencies")} (advisory)`]
      : []),
  ].join(" · ");
  const second = [
    ...(summary.protectedItems > 0 ? [`${formatInteger(summary.protectedItems)} protected`] : []),
    ...(summary.diagnostics.total > 0
      ? [`${formatCount(summary.diagnostics.total, "diagnostic")}${summary.diagnostics.actionable > 0
        ? ` (${formatInteger(summary.diagnostics.actionable)} actionable)`
        : ""}`]
      : []),
  ].join(" · ");
  const lines = [first];
  if (second.length > 0) lines.push(second);
  if (model.report.mode === "branch" && model.report.preexisting) {
    const pre = model.report.preexisting;
    if (pre.symbols > 0 || pre.files > 0 || pre.lines > 0) {
      lines.push(`Already dead before this branch: ${formatCount(pre.symbols, "symbol")}, ${formatCount(pre.files, "file")}`);
    }
  }
  return lines.map((line) => truncateEnd(line, width));
}

function rowCounts(row: TuiRow): string {
  switch (row.kind) {
    case "group":
      return `${formatCount(row.removableLines, "line")} · ${formatCount(row.removableSymbols, "symbol")} · ${formatCount(row.removableFiles, "file")}`;
    case "item":
      return row.item.kind === "file"
        ? `whole file · ${formatCount(row.item.lines, "line")}`
        : `${formatCount(row.item.lines, "line")} · ${row.item.confidence}`;
    case "protected":
      return row.item.reason;
    case "dependency":
      return `${row.item.kind} · advisory`;
  }
}

function rowPrefix(row: TuiRow): string {
  switch (row.kind) {
    case "group":
      return "›";
    case "item":
      return " ";
    case "protected":
      return "!";
    case "dependency":
      return "·";
  }
}

function listLines(
  model: TuiModel,
  rows: readonly TuiRow[],
  selection: number,
  width: number,
  height: number,
): string[] {
  const lines: string[] = [];
  const available = Math.max(3, height - 14);
  const start = Math.max(
    0,
    Math.min(rows.length - available, selection - Math.floor(available / 2)),
  );
  const visible = rows.slice(Math.max(0, start), Math.max(0, start) + available);
  for (const [offset, row] of visible.entries()) {
    const index = Math.max(0, start) + offset;
    const marker = index === selection ? "▶" : " ";
    const label = truncatePath(rowLabel(row), Math.max(8, width - 28));
    const counts = rowCounts(row);
    lines.push(truncateEnd(
      `${marker} ${rowPrefix(row)} ${label}  ${counts}`,
      width,
    ));
  }
  if (rows.length > available) {
    lines.push(truncateEnd(`  ${formatInteger(selection + 1)} of ${formatInteger(rows.length)}`, width));
  }
  return lines;
}

export function ReportApp(props: ReportAppProps): ReturnType<typeof h> {
  const stdoutColumns = useStdout().stdout.columns;
  const width = Math.max(40, props.width ?? stdoutColumns ?? 80);
  const height = Math.max(10, props.height ?? 24);
  const { exit } = useApp();
  const [model] = useState(() => buildTuiModel(props.report));
  const [rawSelection, setSelection] = useState(0);
  const [filter, setFilter] = useState("");
  const [filtering, setFiltering] = useState(false);
  const [detail, setDetail] = useState<TuiRow | null>(null);
  const [help, setHelp] = useState(false);
  const [commands, setCommands] = useState(false);
  const [showDiagnostics, setShowDiagnostics] = useState(false);
  const [showProtected, setShowProtected] = useState(false);

  const selected = selectRows(model, filter, rawSelection);
  const rows = selected.rows;
  const selection = selected.selection;

  const move = (delta: number): void => {
    setSelection(clampSelection(rows, selection + delta));
  };

  useInput((input, key) => {
    if (key.ctrl && input === "c") {
      exit();
      return;
    }
    if (filtering) {
      if (key.return) {
        setFiltering(false);
        return;
      }
      if (key.escape) {
        setFilter("");
        setFiltering(false);
        setSelection(0);
        return;
      }
      if (key.backspace || key.delete) {
        setFilter((value) => value.slice(0, -1));
        setSelection(0);
        return;
      }
      if (input.length > 0 && !key.ctrl && !key.meta) {
        setFilter((value) => `${value}${input}`);
        setSelection(0);
      }
      return;
    }
    if (input === "q" || (key.escape && detail === null && !help && !commands)) {
      exit();
      return;
    }
    if (key.escape) {
      if (detail) setDetail(null);
      else if (help) setHelp(false);
      else if (commands) setCommands(false);
      else if (showDiagnostics) setShowDiagnostics(false);
      else if (showProtected) setShowProtected(false);
      return;
    }
    if (input === "?") {
      setHelp((value) => !value);
      return;
    }
    if (input === "/") {
      setFiltering(true);
      return;
    }
    if (input === "j" || key.downArrow) {
      move(1);
      return;
    }
    if (input === "k" || key.upArrow) {
      move(-1);
      return;
    }
    if (input === "g") {
      setSelection(0);
      return;
    }
    if (input === "G") {
      setSelection(Math.max(0, rows.length - 1));
      return;
    }
    if (key.return) {
      const row = rows[selection];
      if (row) setDetail(row);
      return;
    }
    if (input === "c") {
      setCommands((value) => !value);
      return;
    }
    if (input === "r") {
      setDetail(null);
      setHelp(false);
      setCommands(false);
      return;
    }
    if (input === "d") {
      setShowDiagnostics((value) => !value);
      return;
    }
    if (input === "p") {
      setShowProtected((value) => !value);
      return;
    }
  });

  const children: ReturnType<typeof h>[] = [];
  children.push(h(Text, { bold: true, key: "header" }, headerLine(model, width)));
  if (model.report.head.dirty) {
    children.push(h(Text, { key: "dirty" }, truncateEnd("Dirty tree: this report is local feedback, not a CI receipt.", width)));
  }
  children.push(h(Box, { key: "dead", flexDirection: "column", marginTop: 1 },
    ...deadWeightLines(model, width).map((line, index) => h(Text, { key: `dead-${index}` }, line)),
  ));
  children.push(h(Box, { key: "counts", flexDirection: "column", marginTop: 1 },
    ...countLines(model, width).map((line, index) => h(Text, { key: `count-${index}` }, line)),
  ));

  if (help) {
    children.push(h(Box, { key: "help", flexDirection: "column", marginTop: 1, borderStyle: "round", paddingX: 1 },
      h(Text, { bold: true }, "Keys"),
      h(Text, null, "q quit · ↑/↓ move · enter details · ? help"),
      h(Text, null, "/ filter · c next command · r redraw"),
      h(Text, null, "d diagnostics · p protected"),
      h(Text, null, "This view is read-only. It never deletes or analyzes."),
    ));
  } else if (detail) {
    children.push(h(Box, { key: "detail", flexDirection: "column", marginTop: 1, borderStyle: "round", paddingX: 1 },
      h(Text, { bold: true }, "Details"),
      ...detailLines(model, detail).map((line, index) => h(Text, { key: `detail-${index}` }, truncateEnd(line, width - 4))),
      h(Text, { dimColor: true }, "esc back · q quit"),
    ));
  } else if (commands) {
    children.push(h(Box, { key: "commands", flexDirection: "column", marginTop: 1, borderStyle: "round", paddingX: 1 },
      h(Text, { bold: true }, "Next command"),
      ...model.next.map((command, index) => h(Text, { key: `command-${index}` }, truncateEnd(command, width - 4))),
      h(Text, { dimColor: true }, "Press c or esc to close."),
    ));
  } else if (showDiagnostics) {
    children.push(h(Box, { key: "diagnostics", flexDirection: "column", marginTop: 1 },
      ...diagnosticsSummaryLines(model).map((line, index) => h(Text, { key: `diagnostic-${index}` }, truncateEnd(line, width))),
      ...(model.report.diagnostics.groups.length === 0 ? [h(Text, { key: "no-diagnostics" }, "No diagnostics.")] : []),
    ));
  } else if (showProtected) {
    const counts = protectedReasonCounts(model);
    children.push(h(Box, { key: "protected", flexDirection: "column", marginTop: 1 },
      h(Text, { bold: true }, "Protected reasons"),
      ...(counts.length === 0
        ? [h(Text, { key: "no-protected" }, "No protected items.")]
        : counts.map((entry, index) => h(Text, { key: `protected-${index}` }, `${entry.reason}  ${formatInteger(entry.count)}`))),
    ));
  } else {
    children.push(h(Box, { key: "rows", flexDirection: "column", marginTop: 1 },
      ...(rows.length === 0
        ? [
          h(Text, { key: "empty" }, "No removable dead weight found."),
          h(Text, { key: "empty-next" }, `Next: ${model.next[0] ?? "npx getshrike report"}`),
        ]
        : listLines(model, rows, selection, width, height).map((line, index) => h(Text, { key: `row-${index}` }, line))),
    ));
  }

  if (filtering || filter.length > 0) {
    children.push(h(Text, { key: "filter", dimColor: true }, truncateEnd(`Filter: ${filter}${filtering ? "_" : ""}`, width)));
  }
  children.push(h(Text, { key: "footer", dimColor: true }, truncateEnd(
    "q quit · ↑/↓ move · enter details · ? help",
    width,
  )));

  return h(Box, { flexDirection: "column", width }, ...children);
}
