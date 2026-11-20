import type { ChildProcess } from "node:child_process";

export const DEFAULT_TIMEOUT_MS = 300_000;
export const MAX_CAPTURE_BYTES = 1_048_576;

/**
 * Host variables a project check may inherit.
 *
 * Project checks must not receive repository secrets by default. A check that
 * needs a secret declares it in `CheckSpec.env`. PATH/HOME/temp/locale and the
 * Windows system variables are required for ordinary tools to start.
 */
export const SAFE_ENV_KEYS = [
  "PATH",
  "Path",
  "PATHEXT",
  "HOME",
  "USERPROFILE",
  "HOMEDRIVE",
  "HOMEPATH",
  "USER",
  "LOGNAME",
  "SHELL",
  "TMPDIR",
  "TMP",
  "TEMP",
  "SystemRoot",
  "SYSTEMROOT",
  "WINDIR",
  "ComSpec",
  "COMSPEC",
  "LANG",
  "LC_ALL",
  "LC_CTYPE",
  "TERM",
  "NO_COLOR",
  "FORCE_COLOR",
  "CI",
] as const;

/** Build the default environment for a project check. */
export function baseEnvironment(): NodeJS.ProcessEnv {
  const env: NodeJS.ProcessEnv = {};
  for (const key of SAFE_ENV_KEYS) {
    const value = process.env[key];
    if (value !== undefined) env[key] = value;
  }
  return env;
}

export function shellForPlatform(): { file: string; args: string[] } {
  if (process.platform === "win32") {
    return {
      file: process.env.ComSpec ?? "cmd.exe",
      args: ["/d", "/s", "/c"],
    };
  }
  return { file: "/bin/sh", args: ["-c"] };
}

export function killProcess(child: ChildProcess): void {
  if (child.exitCode !== null || child.signalCode !== null) return;
  try {
    if (process.platform !== "win32" && child.pid !== undefined) {
      process.kill(-child.pid, "SIGTERM");
    } else {
      child.kill("SIGTERM");
    }
  } catch {
    // The process may already be gone.
  }
  const timer = setTimeout(() => {
    try {
      if (process.platform !== "win32" && child.pid !== undefined) {
        process.kill(-child.pid, "SIGKILL");
      } else {
        child.kill("SIGKILL");
      }
    } catch {
      // Nothing else to do.
    }
  }, 1_000);
  timer.unref();
}
