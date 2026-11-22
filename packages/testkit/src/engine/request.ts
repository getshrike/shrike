import { spawn } from "node:child_process";

import { locateEngine, type LocateEngineOptions } from "./locate.ts";
import { assertProtocolResponse, parseResponseLine } from "./protocol.ts";
import { EngineProtocolError, PROTOCOL_VERSION, type ProtocolRequest, type ProtocolResponse } from "./types.ts";

export interface EngineRequestOptions extends LocateEngineOptions {
  enginePath?: string;
  timeoutMs?: number;
  env?: NodeJS.ProcessEnv;
  /** Request id; defaults to a deterministic counter-based id. */
  id?: string;
}

let requestCounter = 0;

function nextRequestId(): string {
  requestCounter += 1;
  return `req_${String(requestCounter).padStart(6, "0")}`;
}

/** Spawn the engine for one request and read exactly one response line. */
export function requestEngine<T = unknown>(
  command: string,
  payload: unknown,
  options: EngineRequestOptions = {},
): Promise<ProtocolResponse<T>> {
  const enginePath = options.enginePath ?? locateEngine(options);
  const id = options.id ?? nextRequestId();
  const timeoutMs = options.timeoutMs ?? 120_000;
  const request: ProtocolRequest = {
    protocolVersion: PROTOCOL_VERSION,
    id,
    command,
    payload,
  };

  return new Promise<ProtocolResponse<T>>((resolvePromise, reject) => {
    const child = spawn(enginePath, [], {
      cwd: options.cwd,
      env: { ...process.env, ...options.env },
      stdio: ["pipe", "pipe", "pipe"],
      windowsHide: true,
    });
    let stdout = "";
    let stderr = "";
    let settled = false;
    let timedOut = false;

    const finish = (error: Error | null, response?: ProtocolResponse<T>): void => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      if (error) reject(error);
      else if (response) resolvePromise(response);
    };

    const timer = setTimeout(() => {
      timedOut = true;
      child.kill("SIGKILL");
    }, timeoutMs);
    timer.unref();

    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk: string) => {
      stdout += chunk;
    });
    child.stderr.on("data", (chunk: string) => {
      stderr += chunk;
    });
    child.on("error", (error) => {
      finish(
        new EngineProtocolError(`could not start shrike-engine at ${enginePath}: ${error.message}`),
      );
    });
    child.on("close", (code) => {
      if (timedOut) {
        finish(new EngineProtocolError(`engine timed out after ${timeoutMs} ms`));
        return;
      }
      const lines = stdout.split(/\r?\n/).filter((line) => line.trim().length > 0);
      if (lines.length === 0) {
        finish(
          new EngineProtocolError(
            `engine exited ${String(code)} without a response` +
              (stderr.trim() ? `: ${stderr.trim().slice(0, 500)}` : ""),
          ),
        );
        return;
      }
      if (lines.length > 1) {
        finish(new EngineProtocolError(`engine wrote ${lines.length} response lines; expected 1`));
        return;
      }
      try {
        const parsed = parseResponseLine(lines[0] as string);
        finish(null, assertProtocolResponse(parsed, id) as ProtocolResponse<T>);
      } catch (error) {
        finish(
          error instanceof Error
            ? error
            : new EngineProtocolError(String(error)),
        );
      }
    });

    child.stdin.write(`${JSON.stringify(request)}\n`);
    child.stdin.end();
  });
}
