import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";

export interface CliInvocation {
  command: string;
  args: string[];
}

/**
 * Resolve the CLI without a shell.
 *
 * Prefer a project-local `getshrike` install so the Action uses the same
 * version as the workflow. `SHRIKE_CLI` is an explicit override for global or
 * unusual installs. `shrike` on PATH is the last fallback.
 */
export function resolveCli(root: string): CliInvocation {
  const configured = process.env.SHRIKE_CLI;
  if (configured) {
    return /\.(?:c|m)?js$/i.test(configured)
      ? { command: process.execPath, args: [configured] }
      : { command: configured, args: [] };
  }
  try {
    const require = createRequire(join(root, "package.json"));
    const manifest = require.resolve("getshrike/package.json");
    const launcher = join(dirname(manifest), "bin", "shrike.mjs");
    if (existsSync(launcher)) return { command: process.execPath, args: [launcher] };
  } catch {
    // Not installed in the workspace; fall through to PATH.
  }
  const probe = spawnSync(process.platform === "win32" ? "where" : "which", ["shrike"], {
    cwd: root,
    encoding: "utf8",
  });
  if (probe.status === 0) return { command: "shrike", args: [] };
  throw new Error(
    "shrike was not found. Run `npm install -D getshrike` before this action, " +
      "or set SHRIKE_CLI to the launcher path.",
  );
}
