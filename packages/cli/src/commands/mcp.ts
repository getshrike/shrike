import { existsSync } from "node:fs";
import { join } from "node:path";
import { createInterface } from "node:readline";

import type { ParsedArgs } from "../args.ts";
import { ExitCode, UsageError } from "../errors.ts";
import { safeRelativePath } from "../recovery.ts";
import { VERSION } from "../version.ts";

export const MCP_MAX_OUTPUT = 256 * 1024;

export function validateMcpArgument(name: string, value: string): void {
  if (name === "base") {
    if (
      value.startsWith("-") ||
      value.includes("..") ||
      !/^[A-Za-z0-9._/@{}^~:-]+$/.test(value)
    ) {
      throw new UsageError(`invalid base ref: ${value}`);
    }
    return;
  }
  if (name === "symbol") {
    if (!/^[^#]+#[^#]+$/.test(value)) throw new UsageError("symbol must look like path#Name");
    safeRelativePath(value.slice(0, value.indexOf("#")));
    return;
  }
  if (name === "id" && !/^rcpt_[A-Za-z0-9]{8,64}$/.test(value)) {
    throw new UsageError("receipt id must look like rcpt_<hex>");
  }
}

export async function mcpCommand(root: string, args: ParsedArgs): Promise<number> {
  if (args.positionals[0] !== "serve") throw new UsageError("usage: shrike mcp serve");
  const bundled = join(import.meta.dirname, "main.js");
  const cli = existsSync(bundled) ? bundled : join(import.meta.dirname, "..", "main.ts");
  const rl = createInterface({ input: process.stdin, crlfDelay: Infinity });
  for await (const line of rl) {
    if (!line.trim()) continue;
    let request: { id?: unknown; method?: string; params?: { name?: string; arguments?: unknown } };
    try { request = JSON.parse(line); } catch { continue; }
    const respond = (result: unknown) => process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: request.id ?? null, result })}\n`);
    const fail = (code: number, message: string) =>
      process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: request.id ?? null, error: { code, message } })}\n`);
    if (request.method === "initialize") {
      respond({ protocolVersion: "2024-11-05", capabilities: { tools: {} }, serverInfo: { name: "shrike", version: VERSION } });
    } else if (request.method === "tools/list") {
      respond({ tools: [
        { name: "shrike_delta", description: "Whole-graph PR orphan delta", inputSchema: { type: "object", properties: { base: { type: "string" } } } },
        { name: "shrike_plan", description: "Verified deletion batches", inputSchema: { type: "object", properties: { base: { type: "string" } } } },
        { name: "shrike_proof", description: "Witness or proof trace", inputSchema: { type: "object", properties: { symbol: { type: "string" } }, required: ["symbol"] } },
        { name: "shrike_receipt", description: "Show or list verified deletion receipts", inputSchema: { type: "object", properties: { id: { type: "string" } } } },
      ] });
    } else if (request.method === "tools/call") {
      const name = request.params?.name ?? "";
      const command = name.replace(/^shrike_/, "");
      if (!["delta", "plan", "proof", "receipt"].includes(command)) {
        fail(-32601, "unknown tool");
        continue;
      }
      const toolArgs = (request.params?.arguments ?? {}) as Record<string, unknown>;
      try {
        const argv = [process.execPath, cli, command, "--json"];
        if (typeof toolArgs.base === "string") {
          validateMcpArgument("base", toolArgs.base);
          argv.push("--base", toolArgs.base);
        }
        if (typeof toolArgs.symbol === "string") {
          validateMcpArgument("symbol", toolArgs.symbol);
          argv.push(toolArgs.symbol);
        }
        if (typeof toolArgs.id === "string") {
          validateMcpArgument("id", toolArgs.id);
          argv.push(toolArgs.id);
        }
        const output = await runCli(root, argv);
        const text = output.stdout.length > MCP_MAX_OUTPUT
          ? `${output.stdout.slice(0, MCP_MAX_OUTPUT)}\n[truncated at ${MCP_MAX_OUTPUT} bytes]`
          : output.stdout;
        respond({ content: [{ type: "text", text }], isError: output.code !== 0 });
      } catch (error) {
        fail(-32602, error instanceof Error ? error.message : String(error));
      }
    } else if (request.id !== undefined) {
      fail(-32601, "method not found");
    }
  }
  return ExitCode.OK;
}

export async function runCli(root: string, argv: string[]): Promise<{ code: number; stdout: string }> {
  const { spawn } = await import("node:child_process");
  return new Promise((resolveRun) => {
    const child = spawn(argv[0]!, argv.slice(1), { cwd: root, stdio: ["ignore", "pipe", "inherit"] });
    let stdout = "";
    child.stdout.setEncoding("utf8");
    child.stdout.on("data", (chunk: string) => { stdout += chunk; });
    child.on("close", (code) => resolveRun({ code: code ?? 4, stdout }));
  });
}
