import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { accessSync, constants, existsSync, readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join, resolve } from "node:path";
import { EngineUnavailableError } from "./errors.ts";
import { logTiming } from "./timing.ts";

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

const PLATFORM_PACKAGES: Record<string, string> = {
  "linux-x64": "getshrike-linux-x64",
  "linux-arm64": "getshrike-linux-arm64",
  "darwin-x64": "getshrike-darwin-x64",
  "darwin-arm64": "getshrike-darwin-arm64",
  "win32-x64": "getshrike-win32-x64",
};

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/**
 * Verify a SHA-256 sidecar before executing an engine.
 *
 * Repo builds and `SHRIKE_ENGINE` overrides only need a sidecar when one is
 * present. Packaged platform binaries require one: a missing or malformed
 * checksum is a broken install, not a reason to run an unverified binary.
 */
function verifyChecksum(binary: string, required: boolean): void {
  const sidecar = `${binary}.sha256`;
  if (!existsSync(sidecar)) {
    if (required) {
      throw new EngineUnavailableError(
        `engine checksum sidecar is missing: ${sidecar}. Reinstall getshrike.`,
      );
    }
    return;
  }
  let expected: string;
  try {
    expected = (readFileSync(sidecar, "utf8").trim().split(/\s+/)[0] ?? "").toLowerCase();
  } catch (error) {
    throw new EngineUnavailableError(
      `could not read engine checksum ${sidecar}: ${errorMessage(error)}`,
    );
  }
  if (!/^[0-9a-f]{64}$/.test(expected)) {
    throw new EngineUnavailableError(`engine checksum file is malformed: ${sidecar}`);
  }
  let actual: string;
  try {
    actual = createHash("sha256").update(readFileSync(binary)).digest("hex");
  } catch (error) {
    throw new EngineUnavailableError(
      `could not read engine binary ${binary}: ${errorMessage(error)}`,
    );
  }
  if (actual !== expected) {
    throw new EngineUnavailableError(
      `engine checksum mismatch for ${binary}: expected ${expected}, got ${actual}. Reinstall getshrike.`,
    );
  }
}

/** Resolve the platform package installed by npm, if the host has one. */
function platformPackageEngine(): { binary: string } | { error: string } {
  const key = `${process.platform}-${process.arch}`;
  const name = PLATFORM_PACKAGES[key];
  if (!name) {
    return {
      error: `unsupported platform ${key}; supported: ${Object.keys(PLATFORM_PACKAGES).join(", ")}`,
    };
  }
  let manifest: string;
  try {
    manifest = createRequire(import.meta.url).resolve(`${name}/package.json`);
  } catch {
    return { error: `optional package ${name} is not installed` };
  }
  const binary = join(dirname(manifest), "bin", executable("shrike-engine"));
  if (!isExecutable(binary)) {
    return { error: `optional package ${name} is installed but ${binary} is missing or not executable` };
  }
  // A packaged binary is verified before every run. A mismatch is a hard
  // failure; it is never downgraded to a PATH fallback.
  verifyChecksum(binary, true);
  return { binary };
}

/**
 * Locate the native engine. Fails closed with exit code 4 when absent.
 *
 * Order: `SHRIKE_ENGINE`, a repository `target/{release,debug}` build, the
 * installed platform package, then `PATH`.
 */
export function findEngine(): string {
  const problems: string[] = [];
  const configured = process.env.SHRIKE_ENGINE;
  if (configured) {
    if (isExecutable(configured)) {
      verifyChecksum(configured, false);
      return configured;
    }
    problems.push(`SHRIKE_ENGINE is set to ${configured}, but it is not executable`);
  }
  let current = resolve(process.cwd());
  for (;;) {
    for (const profile of ["release", "debug"]) {
      const candidate = join(current, "target", profile, executable("shrike-engine"));
      if (isExecutable(candidate)) {
        verifyChecksum(candidate, false);
        return candidate;
      }
    }
    const parent = dirname(current);
    if (parent === current) break;
    current = parent;
  }
  const packaged = platformPackageEngine();
  if ("binary" in packaged) return packaged.binary;
  problems.push(packaged.error);
  for (const directory of (process.env.PATH ?? "").split(process.platform === "win32" ? ";" : ":")) {
    if (!directory) continue;
    const candidate = join(directory, executable("shrike-engine"));
    if (isExecutable(candidate)) {
      verifyChecksum(candidate, false);
      return candidate;
    }
  }
  throw new EngineUnavailableError(
    "shrike-engine was not found. Install getshrike with npm (npm install -D getshrike), " +
      "set SHRIKE_ENGINE, or run `cargo build -p shrike-engine`." +
      (problems.length > 0 ? ` Details: ${problems.join("; ")}.` : ""),
  );
}

let counter = 0;

interface EngineRun {
  id: string;
  stdout: Buffer;
  stderr: string;
  exitCode: number | null;
}

/**
 * Spawn the engine, send one request, and collect its output. This is shared by
 * the parsed and raw response paths so timeout and protocol handling cannot
 * drift between them.
 */
async function runEngine(command: string, payload: unknown): Promise<EngineRun> {
  const started = performance.now();
  const binary = findEngine();
  const id = `host_${process.pid}_${++counter}`;
  const request: EngineRequest = { protocolVersion: "1.0", id, command, payload };
  const child = spawn(binary, [], { stdio: ["pipe", "pipe", "pipe"] });
  // Scan responses reach tens of megabytes. Concatenating decoded chunks
  // rebuilds the string on every write and leaves a trail of intermediates for
  // the GC; collecting buffers and decoding once is linear and keeps UTF-8
  // sequences intact across chunk boundaries.
  const stdoutChunks: Buffer[] = [];
  const stderrChunks: Buffer[] = [];
  child.stdout.on("data", (chunk: Buffer) => { stdoutChunks.push(chunk); });
  child.stderr.on("data", (chunk: Buffer) => { stderrChunks.push(chunk); });
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
  logTiming(`engine_${command}_roundtrip`, started);
  const stdout = Buffer.concat(stdoutChunks);
  const stderr = Buffer.concat(stderrChunks).toString("utf8");
  return { id, stdout, stderr, exitCode };
}

/** Extract the single non-empty response line and validate its envelope. */
function responseLine(run: EngineRun): Buffer {
  let start = 0;
  while (start < run.stdout.length) {
    const newline = run.stdout.indexOf(0x0a, start);
    const end = newline === -1 ? run.stdout.length : newline;
    if (end > start) return run.stdout.subarray(start, end);
    start = end + 1;
  }
  throw new EngineUnavailableError(
    `engine produced no response (exit ${String(run.exitCode)})${run.stderr ? `: ${run.stderr.trim()}` : ""}`,
  );
}

/**
 * Parse a response line and return its result. Scan callers that need the raw
 * JSON use `requestEngineRaw` and only fall back to this when the response does
 * not carry the compact dependency projection.
 */
export function parseEngineLine<T = unknown>(line: string | Buffer, id: string): T {
  const parseStarted = performance.now();
  const text = typeof line === "string" ? line : line.toString("utf8");
  let response: EngineResponse;
  try {
    response = JSON.parse(text) as EngineResponse;
  } catch {
    throw new EngineUnavailableError(`engine returned invalid JSON: ${text.slice(0, 200)}`);
  }
  logTiming("engine_response_parse", parseStarted);
  if (typeof response !== "object" || response === null) {
    throw new EngineUnavailableError("engine response was not an object");
  }
  if (response.protocolVersion !== "1.0") {
    throw new EngineUnavailableError(
      `engine response protocol version ${String(response.protocolVersion)} is not supported; expected 1.0`,
    );
  }
  if (response.id !== id) {
    throw new EngineUnavailableError("engine response id did not match the request");
  }
  if (response.ok === true) {
    // A partial result is not a proof. An omitted result must fail closed,
    // exactly like a malformed response.
    if (!("result" in response) || response.result === undefined) {
      throw new EngineUnavailableError("engine response omitted the result payload");
    }
    return response.result as T;
  }
  if (response.ok === false) {
    const error = response.error;
    if (
      typeof error !== "object" || error === null ||
      typeof error.code !== "string" || typeof error.message !== "string"
    ) {
      throw new EngineUnavailableError("engine error response omitted its code or message");
    }
    throw new EngineUnavailableError(`${error.code}: ${error.message}`);
  }
  throw new EngineUnavailableError("engine response did not contain a boolean ok field");
}

/** Send one JSONL request and return the parsed result. */
export async function requestEngine<T = unknown>(
  command: string,
  payload: unknown,
): Promise<T> {
  const run = await runEngine(command, payload);
  return parseEngineLine<T>(responseLine(run), run.id);
}

export interface RawEngineResponse {
  /** The first non-empty response line, still JSON-encoded. */
  line: Buffer;
  /** Request id; pass it to `parseEngineLine` for a full decode. */
  id: string;
}

/**
 * Send one JSONL request and return the response line without decoding it.
 *
 * Large scan responses are tens of megabytes. The CLI only needs a small
 * dependency projection plus the already-serialized result, so decoding the
 * whole envelope would allocate the snapshot twice for no benefit.
 */
export async function requestEngineRaw(
  command: string,
  payload: unknown,
): Promise<RawEngineResponse> {
  const run = await runEngine(command, payload);
  return { line: responseLine(run), id: run.id };
}
