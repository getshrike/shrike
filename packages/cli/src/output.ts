import { stableStringify } from "@shrike/receipts";

/** JSON to stdout only. Human output also goes to stdout; logs go to stderr. */
export function writeJson(value: unknown): void {
  process.stdout.write(`${stableStringify(value, 2)}\n`);
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
