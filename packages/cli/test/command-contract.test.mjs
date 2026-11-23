import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { ENGINE_COMMANDS } from "@shrike/protocol";

const root = fileURLToPath(new URL("../../../", import.meta.url));
const read = (path) => readFileSync(`${root}${path}`, "utf8");

/** CLI commands that are thin projections of one or more engine commands. */
const CLI_ENGINE_COMMANDS = new Map([
  ["version", ["version"]],
  ["scan", ["scan"]],
  ["delta", ["delta"]],
  ["plan", ["plan"]],
  ["proof", ["proof"]],
  ["evidence", ["join-build", "augment-build"]],
]);

/** Deliberately CLI-local commands (workflow, files, or settings only). */
const CLI_LOCAL_ALLOWLIST = new Set([
  "init",
  "deps",
  "prune",
  "verify",
  "receipt",
  "recover",
  "hook",
  "mcp",
  "cache",
]);

/** MCP tools that read receipts are CLI-local; the rest map to engine reads. */
const MCP_ENGINE_TOOLS = new Map([
  ["shrike_delta", "delta"],
  ["shrike_plan", "plan"],
  ["shrike_proof", "proof"],
]);
const MCP_LOCAL_ALLOWLIST = new Set(["shrike_receipt"]);

function switchCases(source) {
  return [...source.matchAll(/case "([a-z-]+)":/g)].map((match) => match[1]);
}

test("protocol command list matches the Rust constant and the JSON Schema", () => {
  const rust = read("crates/shrike-protocol/src/types.rs");
  const block = rust.match(/pub const COMMANDS: &\[&str\] = &\[([\s\S]*?)\];/)[1];
  const rustCommands = [...block.matchAll(/"([a-z-]+)"/g)].map((match) => match[1]);
  assert.deepEqual([...ENGINE_COMMANDS], rustCommands);

  const schema = JSON.parse(read("schemas/protocol.schema.json"));
  const schemaCommands = schema.$defs.request.properties.command.enum;
  assert.deepEqual([...schemaCommands], [...ENGINE_COMMANDS]);
});

test("every CLI command is classified and engine-backed commands exist", () => {
  const cases = switchCases(read("packages/cli/src/main.ts"));
  assert.ok(cases.length > 0, "could not find the CLI command switch");
  for (const command of cases) {
    assert.ok(
      CLI_ENGINE_COMMANDS.has(command) || CLI_LOCAL_ALLOWLIST.has(command),
      `CLI command ${command} is neither engine-backed nor in the local allowlist`,
    );
  }
  for (const [command, engineCommands] of CLI_ENGINE_COMMANDS) {
    assert.ok(cases.includes(command), `engine-backed command ${command} is no longer handled by the CLI`);
    for (const engineCommand of engineCommands) {
      assert.ok(
        ENGINE_COMMANDS.includes(engineCommand),
        `CLI command ${command} maps to unknown engine command ${engineCommand}`,
      );
    }
  }
});

test("MCP tools map to real engine commands or an explicit local allowlist", () => {
  const source = read("packages/cli/src/commands/mcp.ts");
  const tools = [...source.matchAll(/name: "(shrike_[a-z-]+)"/g)].map((match) => match[1]);
  assert.ok(tools.length > 0, "could not find MCP tool definitions");
  for (const tool of tools) {
    if (MCP_ENGINE_TOOLS.has(tool)) {
      assert.ok(
        ENGINE_COMMANDS.includes(MCP_ENGINE_TOOLS.get(tool)),
        `${tool} maps to an unknown engine command`,
      );
    } else {
      assert.ok(MCP_LOCAL_ALLOWLIST.has(tool), `MCP tool ${tool} is not classified`);
    }
  }
});
