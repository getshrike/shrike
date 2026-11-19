import { createHash } from "node:crypto";
import { basename } from "node:path";
import { homedir } from "node:os";

/**
 * Failure normalization and signatures.
 *
 * The signature is a hash of the normalized failure output. It must be stable
 * across runs for a pre-existing failure. The design document names BLAKE3;
 * this package uses SHA-256 to stay dependency-free, with the same
 * `algorithm:hex` shape. Receipts use SHA-256 as well.
 */

export interface NormalizeOptions {
  /** Repository or working directory; occurrences are replaced first. */
  cwd?: string;
  /** Repository root, if different from cwd. */
  root?: string;
  /** Home directory; defaults to the current user's home. */
  homeDir?: string;
  /** Maximum normalized lines retained. Default 400. */
  maxLines?: number;
}

// CSI, OSC, and single-character escapes.
const ANSI_PATTERN =
  /\u001B(?:\][^\u0007]*(?:\u0007|\u001B\\)|\[[0-?]*[ -/]*[@-~]|[@-_])/g;

// Absolute POSIX and Windows paths. The leading slash of a URL path is not
// preceded by a word character or a colon, so `https://` is left alone.
const ABSOLUTE_PATH_PATTERN =
  /(?<![\w:])(?:[A-Za-z]:[\\/]|\/)(?:[^\s:;"'`()<>[\]{}]+[\\/])*[^\s:;"'`()<>[\]{}]+/g;

const DURATION_WORD_PATTERN =
  /\b(?:duration|elapsed|time|took|finished(?:\s+in)?)\s*[:=]?\s*\d+(?:\.\d+)?\s*(?:ns|µs|us|ms|s|secs?|seconds?|m|min|mins|minutes?)\b/gi;

const DURATION_PATTERN =
  /\b\d+(?:\.\d+)?\s*(?:ns|µs|us|ms|secs?|seconds?|mins?|minutes?)\b/gi;

const ISO_TIMESTAMP_PATTERN =
  /\b\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:?\d{2})?\b/g;

const DATE_PATTERN = /\b\d{4}-\d{2}-\d{2}\b/g;

/** Remove ANSI color, cursor, and OSC sequences. */
export function stripAnsi(input: string): string {
  return input.replace(ANSI_PATTERN, "");
}

function replaceLiteralAll(input: string, needle: string): string {
  if (needle.length === 0) return input;
  const forward = needle.replaceAll("\\", "/");
  const backward = forward.replaceAll("/", "\\");
  return input.split(forward).join("").split(backward).join("");
}

/**
 * Normalize check output so the same failure produces the same signature.
 *
 * Removes ANSI, timings, absolute paths, temp paths, and timestamps. Keeps the
 * error type, message, file, line, and test name.
 */
export function normalizeOutput(
  input: string,
  options: NormalizeOptions = {},
): string {
  let text = input.replace(/\r\n?/g, "\n");
  text = stripAnsi(text);
  text = text.replace(/\u0000/g, "");

  const roots = [
    options.cwd,
    options.root,
    options.homeDir ?? homedir(),
  ].filter((value): value is string => Boolean(value && value.length > 1));

  for (const root of roots) {
    text = replaceLiteralAll(text, root);
  }

  text = text.replace(ABSOLUTE_PATH_PATTERN, (match) => {
    const normalized = match.replaceAll("\\", "/");
    return basename(normalized);
  });

  text = text.replace(ISO_TIMESTAMP_PATTERN, "<timestamp>");
  text = text.replace(DATE_PATTERN, "<date>");
  text = text.replace(DURATION_WORD_PATTERN, "<duration>");
  text = text.replace(DURATION_PATTERN, "<duration>");

  const maxLines = options.maxLines ?? 400;
  const lines: string[] = [];
  for (const rawLine of text.split("\n")) {
    const line = rawLine.replace(/[ \t]+/g, " ").trim();
    if (line.length === 0) continue;
    if (lines.length > 0 && lines[lines.length - 1] === line) continue;
    lines.push(line);
    if (lines.length >= maxLines) break;
  }
  return lines.join("\n");
}

/** SHA-256 of arbitrary text as `sha256:<hex>`. */
export function hashText(input: string): string {
  const digest = createHash("sha256").update(input, "utf8").digest("hex");
  return `sha256:${digest}`;
}

/** Stable failure signature for a raw output string. */
export function failureSignature(
  input: string,
  options: NormalizeOptions = {},
): string {
  return hashText(normalizeOutput(input, options));
}

/** Cap raw output before storing it in a result or receipt. */
export function capOutput(input: string, maxBytes = 8_192): string {
  if (Buffer.byteLength(input, "utf8") <= maxBytes) return input;
  const truncated = Buffer.from(input, "utf8").subarray(0, maxBytes).toString("utf8");
  return `${truncated}\n[output truncated]`;
}
