import { createHash } from "node:crypto";
import { closeSync, openSync, readSync } from "node:fs";
import { StringDecoder } from "node:string_decoder";

import { createBuilder } from "./builder.ts";
import { isRecord, sha256 } from "./paths.ts";
import { BuildEvidenceError, type BuildEvidence } from "./types.ts";

/**
 * Normalize one JSON object or a JSONL record stream from an in-memory string.
 * Unknown record families are counted; malformed JSON is not.
 */
export function normalizeBuildEvidence(
  root: string,
  text: string,
  sourcePath: string,
): BuildEvidence {
  const builder = createBuilder(root, sourcePath);
  const parsed: unknown = (() => {
    try {
      return JSON.parse(text);
    } catch {
      return undefined;
    }
  })();

  if (parsed !== undefined) {
    const records = Array.isArray(parsed) ? parsed : [parsed];
    for (const [index, record] of records.entries()) {
      if (!isRecord(record)) {
        throw new BuildEvidenceError("malformed", `record ${index + 1} is not an object: ${sourcePath}`);
      }
      builder.consume(record, index + 1);
    }
  } else {
    const lines = text.split(/\r?\n/);
    for (const [index, line] of lines.entries()) {
      if (!line.trim()) continue;
      let record: unknown;
      try {
        record = JSON.parse(line);
      } catch {
        throw new BuildEvidenceError("malformed", `invalid JSON at ${sourcePath}:${index + 1}`);
      }
      if (!isRecord(record)) {
        throw new BuildEvidenceError("malformed", `record is not an object at ${sourcePath}:${index + 1}`);
      }
      builder.consume(record, index + 1);
    }
  }
  return builder.finish(sha256(text).slice("sha256:".length));
}

/**
 * Stream a JSONL export in bounded chunks. The file is never loaded whole, so a
 * 200 MB export costs one chunk plus the current line.
 */
export function streamBuildEvidence(root: string, path: string): BuildEvidence {
  const builder = createBuilder(root, path);
  const hash = createHash("sha256");
  const decoder = new StringDecoder("utf8");
  const buffer = Buffer.alloc(64 * 1024);
  const fd = openSync(path, "r");
  let remainder = "";
  let lineNumber = 0;
  try {
    for (;;) {
      const bytes = readSync(fd, buffer, 0, buffer.length, null);
      if (bytes === 0) break;
      const chunk = buffer.subarray(0, bytes);
      hash.update(chunk);
      remainder += decoder.write(chunk);
      for (;;) {
        const newline = remainder.indexOf("\n");
        if (newline < 0) break;
        const line = remainder.slice(0, newline).replace(/\r$/, "");
        remainder = remainder.slice(newline + 1);
        lineNumber += 1;
        if (!line.trim()) continue;
        let record: unknown;
        try {
          record = JSON.parse(line);
        } catch {
          throw new BuildEvidenceError("malformed", `invalid JSON at ${path}:${lineNumber}`);
        }
        if (!isRecord(record)) {
          throw new BuildEvidenceError("malformed", `record is not an object at ${path}:${lineNumber}`);
        }
        builder.consume(record, lineNumber);
      }
    }
    remainder += decoder.end();
    if (remainder.trim()) {
      lineNumber += 1;
      let record: unknown;
      try {
        record = JSON.parse(remainder);
      } catch {
        throw new BuildEvidenceError("malformed", `invalid JSON at ${path}:${lineNumber}`);
      }
      if (!isRecord(record)) {
        throw new BuildEvidenceError("malformed", `record is not an object at ${path}:${lineNumber}`);
      }
      builder.consume(record, lineNumber);
    }
  } finally {
    closeSync(fd);
  }
  return builder.finish(hash.digest("hex"));
}
