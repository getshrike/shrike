import { chmodSync, existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

export interface FakeEngineResponse {
  result?: unknown;
  error?: { code: string; message: string; details?: unknown };
  /** Emit this raw text instead of an envelope. */
  raw?: string;
  delayMs?: number;
}

export interface FakeEngineOptions {
  /** Per-command canned responses. Unknown commands return an error. */
  responses?: Record<string, FakeEngineResponse>;
  /** Optional default result for every command. */
  defaultResult?: unknown;
  /** Exit code after handling one request. */
  exitCode?: number;
}

/**
 * Write an executable fake engine for CLI tests.
 *
 * The fake speaks the same JSONL contract as the Rust engine; it does not
 * analyze anything. Tests should assert the host fails closed on malformed
 * responses, timeouts, and engine crashes.
 */
export function writeFakeEngine(
  directory: string,
  options: FakeEngineOptions = {},
): { path: string; cleanup: () => void } {
  mkdirSync(directory, { recursive: true });
  const path = join(directory, "shrike-engine");
  const payload = JSON.stringify({
    responses: options.responses ?? {},
    defaultResult: options.defaultResult,
    exitCode: options.exitCode ?? 0,
  });
  const source = `#!/usr/bin/env node
const config = ${payload};
let buffer = "";
process.stdin.setEncoding("utf8");
process.stdin.on("data", (chunk) => { buffer += chunk; });
process.stdin.on("end", async () => {
  const lines = buffer.split(/\\r?\\n/).filter((line) => line.trim().length > 0);
  if (lines.length === 0) process.exit(1);
  const request = JSON.parse(lines[0]);
  const canned = config.responses[request.command];
  const delay = canned && typeof canned.delayMs === "number" ? canned.delayMs : 0;
  if (delay > 0) await new Promise((resolve) => setTimeout(resolve, delay));
  if (canned && typeof canned.raw === "string") {
    process.stdout.write(canned.raw);
    process.exit(config.exitCode);
  }
  if (canned && canned.error) {
    process.stdout.write(JSON.stringify({
      protocolVersion: "1.0",
      id: request.id,
      ok: false,
      error: canned.error,
    }) + "\\n");
    process.exit(config.exitCode);
  }
  if (!canned && config.defaultResult === undefined) {
    process.stdout.write(JSON.stringify({
      protocolVersion: "1.0",
      id: request.id,
      ok: false,
      error: { code: "unknown_command", message: "no fake response for " + request.command, details: [] },
    }) + "\\n");
    process.exit(config.exitCode);
    return;
  }
  const result = canned && "result" in canned ? canned.result : config.defaultResult;
  process.stdout.write(JSON.stringify({
    protocolVersion: "1.0",
    id: request.id,
    ok: true,
    result,
  }) + "\\n");
  process.exit(config.exitCode);
});
`;
  writeFileSync(path, source, "utf8");
  if (process.platform !== "win32") chmodSync(path, 0o755);
  return {
    path,
    cleanup: () => {
      if (existsSync(path)) rmSync(path, { force: true });
    },
  };
}
