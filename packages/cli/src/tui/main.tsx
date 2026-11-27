/**
 * Ink entry point. This file is bundled separately as `dist/tui.js`; it is
 * never imported by the CLI bundle.
 */
import { render } from "ink";
import { createElement } from "react";

import { ReportApp } from "./app.tsx";
import type { TuiRunOptions } from "./load.ts";
import type { CleanupReport } from "../report/types.ts";

export async function runTui(report: CleanupReport, options: TuiRunOptions): Promise<number> {
  const instance = render(
    createElement(ReportApp, { report, width: options.width }),
    { exitOnCtrlC: false },
  );
  await instance.waitUntilExit();
  return 0;
}
