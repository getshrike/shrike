import { booleanFlag, hasFlag, rejectUnknownFlags, stringFlag, type ParsedArgs } from "../args.ts";
import { engineConfig, loadConfig } from "../config.ts";
import { analyzeDependencies } from "../dependencies.ts";
import { summarizeDiagnostics } from "../diagnostics.ts";
import { headCommit, isDirty } from "../git.ts";
import { ExitCode, UsageError } from "../errors.ts";
import { requestEngine } from "../engine.ts";
import { writeHuman, writeJson, writeStderr } from "../output.ts";
import { analyze, analyzePair } from "../session.ts";
import { buildBranchReport, buildCurrentReport, reportExitCode } from "../report/build.ts";
import { colorEnabled, renderReport, resolveWidth } from "../report/render.ts";
import { loadTuiModule } from "../tui/load.ts";
import type { Plan } from "../cli-types.ts";
import type { CleanupReport } from "../report/types.ts";

const REPORT_FLAGS = [
  "base",
  "json",
  "verbose",
  "quiet",
  "no-tui",
  "width",
  "allow-dirty",
  "help",
];

function parseWidth(args: ParsedArgs): number | undefined {
  const raw = stringFlag(args, "width");
  if (raw === undefined) {
    if (hasFlag(args, "width")) throw new UsageError("--width requires a value");
    return undefined;
  }
  if (!/^\d+$/.test(raw)) throw new UsageError(`invalid --width value: ${raw}`);
  return Number.parseInt(raw, 10);
}

/**
 * Build the cleanup report without writing it. One analysis per invocation:
 * current scope calls `analyze`; branch scope calls `analyzePair` and reuses its
 * head snapshot and delta for the plan.
 */
export async function buildCleanupReport(
  root: string,
  args: ParsedArgs,
): Promise<CleanupReport> {
  if (args.positionals.length > 0) {
    throw new UsageError(`unexpected argument: ${args.positionals[0]}`);
  }
  rejectUnknownFlags(args, REPORT_FLAGS);
  if (booleanFlag(args, "verbose") && booleanFlag(args, "quiet")) {
    throw new UsageError("--verbose and --quiet cannot be used together");
  }
  if (booleanFlag(args, "quiet")) {
    // Parsed here so both report modes agree on whether quiet is valid.
    void booleanFlag(args, "quiet");
  }
  const base = stringFlag(args, "base");
  if (base !== undefined && base.length === 0) {
    throw new UsageError("--base requires a non-empty ref");
  }
  if (base === undefined) {
    const config = await loadConfig(root);
    const snapshot = await analyze(root, config);
    const dependencies = analyzeDependencies(root, snapshot, config.ignoreDependencies);
    const diagnostics = summarizeDiagnostics(snapshot.diagnostics ?? []);
    return buildCurrentReport({
      snapshot,
      headCommit: headCommit(root),
      dirty: isDirty(root),
      dependencies,
      diagnostics,
    });
  }

  const pair = await analyzePair(root, args, {
    extraFlags: ["verbose", "quiet", "no-tui", "width"],
  });
  try {
    const config = await loadConfig(root);
    const plan = await requestEngine<Plan>("plan", {
      snapshot: pair.head,
      delta: pair.delta,
      config: engineConfig(config),
    });
    const dependencies = analyzeDependencies(root, pair.head, config.ignoreDependencies);
    const diagnostics = summarizeDiagnostics(pair.head.diagnostics ?? []);
    return buildBranchReport({
      head: pair.head,
      delta: pair.delta,
      plan,
      baseRef: pair.baseRef,
      baseCommit: pair.baseCommit,
      headCommit: pair.headCommit,
      dirty: pair.dirty,
      dependencies,
      diagnostics,
    });
  } finally {
    pair.cleanup();
  }
}

export interface ReportCommandOptions {
  /** Try the interactive view when the caller has already passed the gate. */
  interactive?: boolean;
}

export async function reportCommand(
  root: string,
  args: ParsedArgs,
  options: ReportCommandOptions = {},
): Promise<number> {
  const width = parseWidth(args);
  const report = await buildCleanupReport(root, args);
  if (booleanFlag(args, "json")) {
    writeJson(report);
    return reportExitCode(report);
  }

  const resolvedWidth = resolveWidth(width, process.stdout.columns, process.env.COLUMNS);
  const color = colorEnabled(Boolean(process.stdout.isTTY), process.env, false);
  const renderOptions = {
    width: resolvedWidth,
    color,
    quiet: booleanFlag(args, "quiet"),
    verbose: booleanFlag(args, "verbose"),
  };
  if (options.interactive === true && !booleanFlag(args, "no-tui")) {
    const tui = await loadTuiModule();
    if (tui) {
      try {
        return await tui.runTui(report, renderOptions);
      } catch (error) {
        if (renderOptions.verbose) {
          writeStderr(`shrike: TUI unavailable: ${error instanceof Error ? error.message : String(error)}`);
          if (process.env.SHRIKE_DEBUG === "1" && error instanceof Error && error.stack) {
            writeStderr(error.stack);
          }
        }
      }
    }
  }
  writeHuman(renderReport(report, renderOptions));
  return reportExitCode(report);
}
