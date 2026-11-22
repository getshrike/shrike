/**
 * Execute a shell command with a hard timeout and bounded output capture.
 *
 * The shell is the trust boundary: project scripts are trusted by definition
 * (docs/09-security-and-trust.md). The verifier runs them only in a worktree.
 */
import { spawn } from "node:child_process";
import type { ChildProcess } from "node:child_process";

import type { RawCommandResult, RunOptions } from "../types.ts";
import {
  DEFAULT_TIMEOUT_MS, MAX_CAPTURE_BYTES, baseEnvironment, killProcess, shellForPlatform,
} from "./environment.ts";

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

export function combinedOutput(raw: RawCommandResult): string {
  const parts = [raw.stdout, raw.stderr].filter((part) => part.length > 0);
  if (raw.spawnError) parts.push(raw.spawnError);
  return parts.join("\n");
}
