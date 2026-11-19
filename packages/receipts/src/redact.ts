import { homedir } from "node:os";

/** Redaction for values that can reach a receipt, log, or PR body. */

export interface RedactOptions {
  /** Repository root; occurrences are replaced with `<repo>`. */
  root?: string;
  /** Home directory; defaults to the current user's home. */
  homeDir?: string;
}

// CSI, OSC, and single-character escapes.
const ANSI_PATTERN =
  /\u001B(?:\][^\u0007]*(?:\u0007|\u001B\\)|\[[0-?]*[ -/]*[@-~]|[@-_])/g;

const TOKEN_PATTERNS: RegExp[] = [
  /\bgh[pousr]_[A-Za-z0-9]{20,}\b/g,
  /\bgithub_pat_[A-Za-z0-9_]{20,}\b/g,
  /\bsk-[A-Za-z0-9_-]{20,}\b/g,
  /\bAKIA[0-9A-Z]{16}\b/g,
  /\bBearer\s+[A-Za-z0-9._~+/=-]{8,}\b/gi,
];

const ENV_SECRET_PATTERN =
  /\b([A-Z][A-Z0-9_]*(?:TOKEN|SECRET|PASSWORD|PASSWD|KEY|CREDENTIAL)[A-Z0-9_]*)=([^\s]+)/g;

function replacePath(input: string, value: string, replacement: string): string {
  if (value.length < 2) return input;
  const forward = value.replaceAll("\\", "/");
  const backward = forward.replaceAll("/", "\\");
  return input
    .split(forward)
    .join(replacement)
    .split(backward)
    .join(replacement);
}

/**
 * Remove control sequences, replace repository and home prefixes, and mask
 * credential-shaped strings. Redaction is conservative: it may redact a value
 * that was not a secret, but it must never leave a known secret in place.
 */
export function redactText(input: string, options: RedactOptions = {}): string {
  let text = input.replace(/\r\n?/g, "\n");
  text = text.replace(ANSI_PATTERN, "");
  text = text.replace(/[\u0000-\u0008\u000B\u000C\u000E-\u001F\u007F]/g, "");

  if (options.root) text = replacePath(text, options.root, "<repo>");
  const home = options.homeDir ?? homedir();
  if (home) text = replacePath(text, home, "~");

  for (const pattern of TOKEN_PATTERNS) text = text.replace(pattern, "[redacted]");
  text = text.replace(ENV_SECRET_PATTERN, "$1=[redacted]");
  return text;
}

/** Redact one command string while preserving its readable shape. */
export function redactCommand(command: string, options: RedactOptions = {}): string {
  return redactText(command, options);
}
