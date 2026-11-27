import type { CleanupReport } from "../report/types.ts";

export interface TuiRunOptions {
  width: number;
  color: boolean;
  quiet: boolean;
  verbose: boolean;
}

export interface TuiModule {
  runTui(report: CleanupReport, options: TuiRunOptions): Promise<number>;
}

/**
 * Load the separately bundled Ink app.
 *
 * The specifier is deliberately non-literal: esbuild must leave this dynamic
 * import alone so `dist/main.js` never contains Ink, React, or Yoga. A missing
 * bundle or a failed import is a fallback, not a command failure.
 */
export async function loadTuiModule(): Promise<TuiModule | null> {
  const specifier = "./tui.js";
  try {
    const imported: unknown = await import(specifier);
    if (typeof imported !== "object" || imported === null) return null;
    const runTui = (imported as { runTui?: unknown }).runTui;
    if (typeof runTui !== "function") return null;
    return imported as TuiModule;
  } catch {
    return null;
  }
}
