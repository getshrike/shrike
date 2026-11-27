import test from "node:test";
import assert from "node:assert/strict";

import { isAgentSession, shouldUseTui } from "../src/tui/gate.ts";

const tty = { stdoutIsTTY: true, stdinIsTTY: true, env: {} };

test("loads the TUI only on a safe interactive TTY", () => {
  assert.equal(shouldUseTui(tty), true);
  assert.equal(shouldUseTui({ ...tty, env: { TERM: "xterm-256color" } }), true);
});

test("each negative condition disables the TUI", () => {
  assert.equal(shouldUseTui({ ...tty, stdoutIsTTY: false }), false);
  assert.equal(shouldUseTui({ ...tty, stdinIsTTY: false }), false);
  assert.equal(shouldUseTui({ ...tty, json: true }), false);
  assert.equal(shouldUseTui({ ...tty, noTui: true }), false);
  for (const env of [
    { CI: "1" },
    { CI: "" },
    { SHRIKE_NO_TUI: "1" },
    { SHRIKE_NO_TUI: "" },
    { SHRIKE_TUI_DISABLE: "1" },
    { TERM: "dumb" },
  ]) {
    assert.equal(shouldUseTui({ ...tty, env }), false, JSON.stringify(env));
  }
  assert.equal(shouldUseTui({ ...tty, columns: 39 }), false);
  assert.equal(shouldUseTui({ ...tty, rows: 9 }), false);
  assert.equal(shouldUseTui({ ...tty, columns: 40, rows: 10 }), true);
});

test("known agent sessions never load the TUI", () => {
  for (const name of [
    "CLAUDECODE",
    "CLAUDE_CODE",
    "CURSOR_AGENT",
    "CURSOR_TRACE_ID",
    "CODEX_SANDBOX",
    "CODEX_AGENT",
    "OPENCODE",
    "OPENCODE_SESSION",
    "GEMINI_CLI",
    "COPILOT_AGENT",
  ]) {
    assert.equal(isAgentSession({ [name]: "" }), true, name);
    assert.equal(shouldUseTui({ ...tty, env: { [name]: "" } }), false, name);
  }
  assert.equal(isAgentSession({}), false);
  assert.equal(isAgentSession({ UNRELATED: "1" }), false);
});
