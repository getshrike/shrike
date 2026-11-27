/**
 * Pure decision for loading the interactive report.
 *
 * A false negative (plain output) is acceptable; loading Ink in a pipe, CI, or
 * an agent session is not. Every input is explicit so the table is testable
 * without a pseudo-TTY.
 */

const AGENT_VARIABLES = [
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
] as const;

export type TuiEnvironment = Readonly<Record<string, string | undefined>>;

export function isAgentSession(env: TuiEnvironment): boolean {
  return AGENT_VARIABLES.some((name) => env[name] !== undefined);
}

export interface TuiGateInput {
  stdoutIsTTY: boolean;
  stdinIsTTY: boolean;
  env: TuiEnvironment;
  json?: boolean;
  noTui?: boolean;
  columns?: number;
  rows?: number;
}

export function shouldUseTui(input: TuiGateInput): boolean {
  if (!input.stdoutIsTTY || !input.stdinIsTTY) return false;
  if (input.json === true || input.noTui === true) return false;
  if (input.env.CI !== undefined) return false;
  if (input.env.SHRIKE_NO_TUI !== undefined) return false;
  if (input.env.SHRIKE_TUI_DISABLE !== undefined) return false;
  if (input.env.TERM === "dumb") return false;
  if (isAgentSession(input.env)) return false;
  if (input.columns !== undefined && input.columns < 40) return false;
  if (input.rows !== undefined && input.rows < 10) return false;
  return true;
}
