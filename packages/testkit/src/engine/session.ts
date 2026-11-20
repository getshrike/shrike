import { spawn } from "node:child_process";

import { assertProtocolResponse, parseResponseLine } from "./protocol.ts";
import { EngineProtocolError, PROTOCOL_VERSION, type ProtocolRequest, type ProtocolResponse } from "./types.ts";

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
