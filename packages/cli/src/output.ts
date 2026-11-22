/**
 * JSON to stdout only. Human output also goes to stdout; logs go to stderr.
 *
 * Scan payloads can be tens of megabytes. `stableStringify` canonicalizes the
 * whole value (recursively rebuilding every object with sorted keys) before
 * serializing it, which doubled the cost of printing large scans. Command
 * payloads are built in deterministic order, so plain `JSON.stringify` keeps
 * output stable without the extra object graph. Receipts still use canonical
 * JSON for hashing and on-disk artifacts.
 */
export function writeJson(value: unknown): void {
  const stringifyStarted = performance.now();
  const text = JSON.stringify(value, null, 2);
  const writeStarted = performance.now();
  process.stdout.write(`${text}\n`);
  if (process.env.SHRIKE_TIMING) {
    const done = performance.now();
    console.error(`shrike cli: stringify=${Math.round(writeStarted - stringifyStarted)}ms`);
    console.error(`shrike cli: write=${Math.round(done - writeStarted)}ms bytes=${text.length}`);
  }
}

/**
 * Write JSON that was already serialized by the engine or assembled by string
 * concatenation. Large scans use this to avoid decoding and re-encoding the
 * snapshot: the bytes are schema-identical, only the whitespace differs.
 *
 * A scan response can be a list of chunks so the serialized snapshot is written
 * straight from the engine's buffer instead of being copied into one string.
 */
export function writeRawJson(content: string | Buffer | Array<string | Buffer>): void {
  const writeStarted = performance.now();
  const parts = Array.isArray(content) ? content : [content];
  const last = parts.at(-1);
  const hasNewline = typeof last === "string"
    ? last.endsWith("\n")
    : last !== undefined && last.length > 0 && last[last.length - 1] === 0x0a;
  if (!hasNewline) parts.push("\n");
  let bytes = 0;
  for (const part of parts) {
    bytes += Buffer.byteLength(part);
    process.stdout.write(part);
  }
  if (process.env.SHRIKE_TIMING) {
    console.error(`shrike cli: write=${Math.round(performance.now() - writeStarted)}ms bytes=${bytes}`);
  }
}

export function writeHuman(text: string): void {
  process.stdout.write(text.endsWith("\n") ? text : `${text}\n`);
}

export function writeStderr(text: string): void {
  process.stderr.write(text.endsWith("\n") ? text : `${text}\n`);
}

/** A single error object for `--json` failures; the human message stays on stderr. */
export function writeJsonError(error: {
  code: string;
  message: string;
  exitCode: number;
  details?: string;
}): void {
  writeJson({
    error: {
      code: error.code,
      message: error.message,
      exitCode: error.exitCode,
      ...(error.details ? { details: error.details } : {}),
    },
  });
}

export function formatCount(count: number, singular: string, plural = `${singular}s`): string {
  return `${count} ${count === 1 ? singular : plural}`;
}
