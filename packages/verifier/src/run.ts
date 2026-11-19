import { spawn } from "node:child_process";
import type { ChildProcess } from "node:child_process";
import { isAbsolute, join } from "node:path";
import { capOutput, failureSignature, normalizeOutput } from "./normalize.ts";
import type {
  CheckResult,
  CheckSpec,
  CheckStatus,
  RawCommandResult,
  RunOptions,
} from "./types.ts";

const DEFAULT_TIMEOUT_MS = 300_000;
const MAX_CAPTURE_BYTES = 1_048_576;

/**
 * Host variables a project check may inherit.
 *
 * Project checks must not receive repository secrets by default. A check that
 * needs a secret declares it in `CheckSpec.env`. PATH/HOME/temp/locale and the
 * Windows system variables are required for ordinary tools to start.
 */
const SAFE_ENV_KEYS = [
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

function shellForPlatform(): { file: string; args: string[] } {
  if (process.platform === "win32") {
    return {
      file: process.env.ComSpec ?? "cmd.exe",
      args: ["/d", "/s", "/c"],
    };
  }
  return { file: "/bin/sh", args: ["-c"] };
}

function killProcess(child: ChildProcess): void {
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

/**
 * Execute a shell command with a hard timeout and bounded output capture.
 *
 * The shell is the trust boundary: project scripts are trusted by definition
 * (docs/09-security-and-trust.md). The verifier runs them only in a worktree.
 */
export function executeCommand(
  command: string,
  options: RunOptions = {},
): Promise<RawCommandResult> {
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const maxBytes = options.maxOutputBytes ?? MAX_CAPTURE_BYTES;
  const shell = shellForPlatform();
  const startedAt = Date.now();

  return new Promise<RawCommandResult>((resolve) => {
    let stdout = "";
    let stderr = "";
    let timedOut = false;
    let spawnError: string | null = null;
    let settled = false;
    let timeoutTimer: NodeJS.Timeout | undefined;

    let child: ChildProcess;
    try {
      child = spawn(shell.file, [...shell.args, command], {
        cwd: options.cwd,
        // Inherit only the check-safe host environment. Secrets are passed
        // explicitly through CheckSpec.env; see docs/09 §2.3.
        env: { ...baseEnvironment(), ...options.env },
        stdio: ["ignore", "pipe", "pipe"],
        detached: process.platform !== "win32",
        windowsHide: true,
      });
    } catch (error) {
      resolve({
        exitCode: null,
        signal: null,
        stdout: "",
        stderr: "",
        durationMs: Date.now() - startedAt,
        timedOut: false,
        spawnError: error instanceof Error ? error.message : String(error),
      });
      return;
    }

    const append = (current: string, chunk: Buffer): string => {
      if (Buffer.byteLength(current, "utf8") >= maxBytes) return current;
      const remaining = maxBytes - Buffer.byteLength(current, "utf8");
      return current + chunk.subarray(0, remaining).toString("utf8");
    };

    child.stdout?.on("data", (chunk: Buffer) => {
      stdout = append(stdout, chunk);
    });
    child.stderr?.on("data", (chunk: Buffer) => {
      stderr = append(stderr, chunk);
    });

    child.on("error", (error) => {
      spawnError = error.message;
    });

    const finish = (exitCode: number | null, signal: NodeJS.Signals | null): void => {
      if (settled) return;
      settled = true;
      if (timeoutTimer) clearTimeout(timeoutTimer);
      resolve({
        exitCode,
        signal,
        stdout,
        stderr,
        durationMs: Date.now() - startedAt,
        timedOut,
        spawnError,
      });
    };

    child.on("close", (code, signal) => {
      finish(code, signal);
    });

    if (timeoutMs > 0) {
      timeoutTimer = setTimeout(() => {
        timedOut = true;
        killProcess(child);
      }, timeoutMs);
      timeoutTimer.unref();
    }
  });
}

function combinedOutput(raw: RawCommandResult): string {
  const parts = [raw.stdout, raw.stderr].filter((part) => part.length > 0);
  if (raw.spawnError) parts.push(raw.spawnError);
  return parts.join("\n");
}

/** Run one check once. Reruns are handled by `runCheckWithReruns`. */
export async function runCheck(
  spec: CheckSpec,
  options: RunOptions = {},
): Promise<CheckResult> {
  const cwd = spec.cwd === undefined
    ? options.cwd
    : isAbsolute(spec.cwd)
      ? spec.cwd
      : join(options.cwd ?? process.cwd(), spec.cwd);
  const raw = await executeCommand(spec.command, {
    ...options,
    cwd,
    timeoutMs: spec.timeoutMs ?? options.timeoutMs,
    env: { ...options.env, ...spec.env },
  });

  let status: CheckStatus;
  if (raw.spawnError) {
    status = "error";
  } else if (raw.timedOut) {
    status = "timeout";
  } else if (raw.exitCode === 0) {
    status = "pass";
  } else {
    status = "fail";
  }

  const combined = combinedOutput(raw);
  const normalized = normalizeOutput(combined, {
    cwd,
    root: options.cwd,
  });
  const signature =
    status === "pass" ? null : failureSignature(combined, { cwd, root: options.cwd });

  return {
    name: spec.name,
    command: spec.command,
    status,
    exitCode: raw.exitCode,
    durationMs: raw.durationMs,
    output: capOutput(normalized),
    signature,
    timedOut: raw.timedOut,
    reruns: 0,
    flaky: false,
    required: spec.required ?? true,
  };
}

/**
 * Run a check with optional reruns.
 *
 * A failure followed by a pass is `flaky` (never a clean pass). A set of
 * failures with different signatures is also marked flaky, per
 * docs/05-verified-deletion.md §3.2.
 */
export async function runCheckWithReruns(
  spec: CheckSpec,
  options: RunOptions = {},
): Promise<CheckResult> {
  const first = await runCheck(spec, options);
  if (first.status === "pass" || first.status === "skipped") return first;

  const extraRuns = Math.max(0, spec.reruns ?? 0);
  if (extraRuns === 0) return first;

  const failures: CheckResult[] = [first];
  let totalDuration = first.durationMs;

  for (let attempt = 1; attempt <= extraRuns; attempt += 1) {
    const next = await runCheck(spec, options);
    totalDuration += next.durationMs;
    if (next.status === "pass") {
      return {
        ...next,
        durationMs: totalDuration,
        reruns: attempt,
        flaky: true,
        signature: first.signature,
        output: first.output,
      };
    }
    failures.push(next);
  }

  const signatures = new Set(
    failures.map((result) => result.signature).filter((value): value is string => Boolean(value)),
  );
  const last = failures[failures.length - 1] ?? first;
  return {
    ...last,
    durationMs: totalDuration,
    reruns: extraRuns,
    flaky: signatures.size > 1,
    signature: first.signature,
    output: first.output,
  };
}

/** Run checks sequentially so results are deterministic and resource-bounded. */
export async function runChecks(
  specs: CheckSpec[],
  options: RunOptions = {},
): Promise<CheckResult[]> {
  const results: CheckResult[] = [];
  for (const spec of specs) {
    results.push(await runCheckWithReruns(spec, options));
  }
  return results;
}
