import { spawn } from "node:child_process";
import { accessSync, constants } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { EngineUnavailableError } from "./errors.ts";

export interface EngineRequest {
  protocolVersion: "1.0";
  id: string;
  command: string;
  payload: unknown;
}

export interface EngineError {
  code: string;
  message: string;
  details?: string;
}

export type EngineResponse =
  | { protocolVersion: "1.0"; id: string; ok: true; result: unknown }
  | { protocolVersion: "1.0"; id: string; ok: false; error: EngineError };

function executable(name: string): string {
  return process.platform === "win32" ? `${name}.exe` : name;
}

function isExecutable(path: string): boolean {
  try {
    accessSync(path, constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

/** Locate the native engine. Fails closed with exit code 4 when absent. */
export function findEngine(): string {
  const configured = process.env.SHRIKE_ENGINE;
  if (configured && isExecutable(configured)) return configured;
  let current = resolve(process.cwd());
  for (;;) {
    for (const profile of ["release", "debug"]) {
      const candidate = join(current, "target", profile, executable("shrike-engine"));
      if (isExecutable(candidate)) return candidate;
    }
    const parent = dirname(current);
    if (parent === current) break;
    current = parent;
  }
  for (const directory of (process.env.PATH ?? "").split(process.platform === "win32" ? ";" : ":")) {
    const candidate = join(directory, executable("shrike-engine"));
    if (isExecutable(candidate)) return candidate;
  }
  throw new EngineUnavailableError(
    "shrike-engine was not found. Set SHRIKE_ENGINE or run `cargo build -p shrike-engine`.",
  );
}

let counter = 0;

/** Send one JSONL request and return the result. */
export async function requestEngine<T = unknown>(
  command: string,
  payload: unknown,
): Promise<T> {
  const binary = findEngine();
  const id = `host_${process.pid}_${++counter}`;
  const request: EngineRequest = { protocolVersion: "1.0", id, command, payload };
  const child = spawn(binary, [], { stdio: ["pipe", "pipe", "pipe"] });
  let stdout = "";
  let stderr = "";
  child.stdout.setEncoding("utf8");
  child.stderr.setEncoding("utf8");
  child.stdout.on("data", (chunk: string) => { stdout += chunk; });
  child.stderr.on("data", (chunk: string) => { stderr += chunk; });
  child.stdin.write(`${JSON.stringify(request)}\n`);
  child.stdin.end();
  const configured = Number(process.env.SHRIKE_ENGINE_TIMEOUT_MS ?? 60_000);
  const timeoutMs = Number.isFinite(configured) && configured > 0 ? configured : 60_000;
  let timedOut = false;
  const timer = setTimeout(() => {
    timedOut = true;
    child.kill("SIGKILL");
  }, timeoutMs);
  const exitCode = await new Promise<number | null>((resolveExit) => {
    child.on("close", (code) => resolveExit(code));
    child.on("error", () => resolveExit(null));
  });
  clearTimeout(timer);
  if (timedOut) {
    throw new EngineUnavailableError(`engine timed out after ${timeoutMs}ms`);
  }
  const line = stdout.split("\n").find((value) => value.trim().length > 0);
  if (!line) {
    throw new EngineUnavailableError(
      `engine produced no response (exit ${String(exitCode)})${stderr ? `: ${stderr.trim()}` : ""}`,
    );
  }
  let response: EngineResponse;
  try {
    response = JSON.parse(line) as EngineResponse;
  } catch {
    throw new EngineUnavailableError(`engine returned invalid JSON: ${line.slice(0, 200)}`);
  }
  if (response.id !== id) {
    throw new EngineUnavailableError("engine response id did not match the request");
  }
  if (!response.ok) {
    const error = response.error;
    throw new EngineUnavailableError(`${error.code}: ${error.message}`);
  }
  return response.result as T;
}
