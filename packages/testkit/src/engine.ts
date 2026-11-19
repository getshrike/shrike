import { spawn } from "node:child_process";
import { accessSync, constants, existsSync, readFileSync } from "node:fs";
import { delimiter, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/** Protocol version locked by the host/engine contract. */
export const PROTOCOL_VERSION = "1.0";

export interface ProtocolRequest {
  protocolVersion: "1.0";
  id: string;
  command: string;
  payload: unknown;
}

export interface ProtocolErrorPayload {
  code: string;
  message: string;
  details?: unknown;
}

export type ProtocolResponse<T = unknown> =
  | {
      protocolVersion: "1.0";
      id: string;
      ok: true;
      result: T;
    }
  | {
      protocolVersion: "1.0";
      id: string;
      ok: false;
      error: ProtocolErrorPayload;
    };

export class EngineProtocolError extends Error {
  readonly details: string | undefined;

  constructor(message: string, details?: string) {
    super(message);
    this.name = "EngineProtocolError";
    this.details = details;
  }
}

export class EngineNotFoundError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "EngineNotFoundError";
  }
}

function isExecutable(path: string): boolean {
  try {
    accessSync(path, constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

function walkUpForEngine(start: string): string | null {
  let current = resolve(start);
  for (;;) {
    for (const profile of ["release", "debug"]) {
      const candidate = join(current, "target", profile, executableName("shrike-engine"));
      if (isExecutable(candidate)) return candidate;
    }
    const parent = dirname(current);
    if (parent === current) return null;
    current = parent;
  }
}

function executableName(base: string): string {
  return process.platform === "win32" ? `${base}.exe` : base;
}

function findOnPath(name: string): string | null {
  const pathValue = process.env.PATH ?? "";
  for (const directory of pathValue.split(delimiter)) {
    if (!directory) continue;
    const candidate = join(directory, executableName(name));
    if (isExecutable(candidate)) return candidate;
  }
  return null;
}

export interface LocateEngineOptions {
  cwd?: string;
  /** Override the package/repo root used to find target/. */
  repoRoot?: string;
}

/**
 * Locate `shrike-engine`: env SHRIKE_ENGINE, target/release, target/debug, PATH.
 * A set but invalid SHRIKE_ENGINE is an error rather than a silent fallback.
 */
export function locateEngine(options: LocateEngineOptions = {}): string {
  const override = process.env.SHRIKE_ENGINE;
  if (override && override.length > 0) {
    if (!isExecutable(override)) {
      throw new EngineNotFoundError(
        `SHRIKE_ENGINE points to a missing or non-executable file: ${override}`,
      );
    }
    return resolve(override);
  }

  const moduleRepoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
  const candidates = [
    options.repoRoot,
    walkUpForEngine(options.cwd ?? process.cwd()),
    walkUpForEngine(moduleRepoRoot),
  ].filter((value): value is string => Boolean(value));

  for (const root of candidates) {
    for (const profile of ["release", "debug"]) {
      const candidate = join(root, "target", profile, executableName("shrike-engine"));
      if (isExecutable(candidate)) return candidate;
    }
  }

  const onPath = findOnPath("shrike-engine");
  if (onPath) return onPath;

  throw new EngineNotFoundError(
    "could not find shrike-engine (set SHRIKE_ENGINE, build target/release/shrike-engine, or add it to PATH)",
  );
}

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

function parseResponseLine(line: string): unknown {
  try {
    return JSON.parse(line);
  } catch {
    throw new EngineProtocolError(`engine returned malformed JSON: ${line.slice(0, 200)}`);
  }
}

/** Validate the response envelope and return a typed response. */
export function assertProtocolResponse(value: unknown, expectedId?: string): ProtocolResponse {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new EngineProtocolError("engine response is not an object");
  }
  const record = value as Record<string, unknown>;
  if (record.protocolVersion !== PROTOCOL_VERSION) {
    throw new EngineProtocolError(
      `unsupported protocolVersion: ${String(record.protocolVersion)}`,
    );
  }
  if (typeof record.id !== "string") {
    throw new EngineProtocolError("engine response is missing a string id");
  }
  if (expectedId !== undefined && record.id !== expectedId) {
    throw new EngineProtocolError(
      `engine response id mismatch: expected ${expectedId}, received ${record.id}`,
    );
  }
  if (record.ok === true) {
    if (!("result" in record) || record.result === undefined || record.result === null) {
      throw new EngineProtocolError("ok response is missing a result");
    }
    return {
      protocolVersion: PROTOCOL_VERSION,
      id: record.id,
      ok: true,
      result: record.result,
    };
  }
  if (record.ok === false) {
    const error = record.error;
    if (typeof error !== "object" || error === null) {
      throw new EngineProtocolError("error response is missing an error object");
    }
    const errorRecord = error as Record<string, unknown>;
    if (typeof errorRecord.code !== "string" || typeof errorRecord.message !== "string") {
      throw new EngineProtocolError("error response has an invalid code or message");
    }
    return {
      protocolVersion: PROTOCOL_VERSION,
      id: record.id,
      ok: false,
      error: {
        code: errorRecord.code,
        message: errorRecord.message,
        ...(errorRecord.details !== undefined ? { details: errorRecord.details } : {}),
      },
    };
  }
  throw new EngineProtocolError("engine response has a non-boolean ok field");
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

interface PendingRequest {
  resolve: (value: ProtocolResponse) => void;
  reject: (error: Error) => void;
  timer: NodeJS.Timeout;
}

/** Persistent JSONL engine session for tests that need multiple commands. */
export class EngineSession {
  private readonly child: ReturnType<typeof spawn>;
  private buffer = "";
  private readonly pending = new Map<string, PendingRequest>();
  private counter = 0;
  private closed = false;

  constructor(enginePath: string, options: { cwd?: string; env?: NodeJS.ProcessEnv } = {}) {
    this.child = spawn(enginePath, [], {
      cwd: options.cwd,
      env: { ...process.env, ...options.env },
      stdio: ["pipe", "pipe", "pipe"],
      windowsHide: true,
    });
    this.child.stdout!.setEncoding("utf8");
    this.child.stdout!.on("data", (chunk: string) => this.onData(chunk));
    this.child.on("close", () => {
      this.closed = true;
      for (const [id, pending] of this.pending) {
        clearTimeout(pending.timer);
        pending.reject(new EngineProtocolError(`engine closed before responding to ${id}`));
      }
      this.pending.clear();
    });
  }

  private onData(chunk: string): void {
    this.buffer += chunk;
    for (;;) {
      const newline = this.buffer.indexOf("\n");
      if (newline < 0) break;
      const line = this.buffer.slice(0, newline).trim();
      this.buffer = this.buffer.slice(newline + 1);
      if (!line) continue;
      let response: ProtocolResponse;
      try {
        response = assertProtocolResponse(parseResponseLine(line));
      } catch {
        continue;
      }
      const pending = this.pending.get(response.id);
      if (!pending) continue;
      this.pending.delete(response.id);
      clearTimeout(pending.timer);
      pending.resolve(response);
    }
  }

  request<T = unknown>(
    command: string,
    payload: unknown,
    timeoutMs = 120_000,
  ): Promise<ProtocolResponse<T>> {
    if (this.closed) {
      return Promise.reject(new EngineProtocolError("engine session is closed"));
    }
    this.counter += 1;
    const id = `req_${String(this.counter).padStart(6, "0")}`;
    const request: ProtocolRequest = {
      protocolVersion: PROTOCOL_VERSION,
      id,
      command,
      payload,
    };
    return new Promise<ProtocolResponse<T>>((resolvePromise, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new EngineProtocolError(`engine request ${id} timed out after ${timeoutMs} ms`));
      }, timeoutMs);
      timer.unref();
      this.pending.set(id, {
        resolve: (value) => resolvePromise(value as ProtocolResponse<T>),
        reject,
        timer,
      });
      this.child.stdin!.write(`${JSON.stringify(request)}\n`);
    });
  }

  close(): void {
    this.closed = true;
    this.child.stdin!.end();
    this.child.kill("SIGTERM");
  }
}
