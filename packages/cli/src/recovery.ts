import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, isAbsolute, join, relative, resolve } from "node:path";
import { stableStringify } from "@shrike/receipts";

/**
 * Recovery snapshots for a failed revert.
 *
 * A snapshot is written before a batch is applied. It contains the exact bytes
 * of every file the batch can touch, so `shrike recover` can restore the tree
 * even when git checkout or a project script has left it in an unknown state.
 * Snapshots live under `.shrike/recovery/` and are never committed.
 */

export interface RecoveryFile {
  path: string;
  contentHash: string;
  content: string;
}

export interface RecoverySnapshot {
  schemaVersion: "1.0";
  id: string;
  createdAt: string;
  headCommit: string;
  files: RecoveryFile[];
}

export interface RecoveryReference {
  snapshotPath: string;
  restoreCommand: string;
}

export class RecoveryError extends Error {
  readonly snapshotPath: string | null;

  constructor(message: string, snapshotPath: string | null = null) {
    super(message);
    this.name = "RecoveryError";
    this.snapshotPath = snapshotPath;
  }
}

export function contentHash(content: string): string {
  return `sha256:${createHash("sha256").update(content, "utf8").digest("hex")}`;
}

/**
 * Normalize a repository-relative path and reject anything that could escape
 * the repository. The engine is a separate process; its paths are data.
 */
export function safeRelativePath(input: string): string {
  const normalized = input.replaceAll("\\", "/").replace(/^\.\//, "");
  if (normalized.length === 0) throw new RecoveryError("empty file path");
  if (isAbsolute(normalized) || /^[A-Za-z]:\//.test(normalized) || normalized.startsWith("/")) {
    throw new RecoveryError(`absolute file path is not allowed: ${input}`);
  }
  const parts = normalized.split("/");
  if (parts.some((part) => part === "..")) {
    throw new RecoveryError(`path traversal is not allowed: ${input}`);
  }
  const clean = parts.filter((part) => part.length > 0 && part !== ".").join("/");
  if (clean.length === 0) throw new RecoveryError(`invalid file path: ${input}`);
  return clean;
}

/** Resolve a repository-relative path and prove it stays inside the root. */
export function resolveInside(root: string, input: string): string {
  const clean = safeRelativePath(input);
  const base = resolve(root);
  const target = resolve(base, clean);
  const prefix = base.endsWith("/") ? base : `${base}/`;
  if (target !== base && !target.startsWith(prefix)) {
    throw new RecoveryError(`path escapes the repository: ${input}`);
  }
  return target;
}

function recoveryDir(root: string): string {
  return join(root, ".shrike", "recovery");
}

/** Write a snapshot for the named files and return its reference. */
export function writeRecoverySnapshot(
  root: string,
  headCommit: string,
  files: string[],
  now = new Date(),
): RecoveryReference {
  const unique = [...new Set(files.map(safeRelativePath))].sort();
  const entries: RecoveryFile[] = [];
  for (const path of unique) {
    const full = resolveInside(root, path);
    if (!existsSync(full)) continue;
    const content = readFileSync(full, "utf8");
    entries.push({ path, contentHash: contentHash(content), content });
  }
  const identity = stableStringify({ headCommit, files: entries });
  const id = `rec_${createHash("sha256").update(identity, "utf8").digest("hex").slice(0, 16)}`;
  const snapshot: RecoverySnapshot = {
    schemaVersion: "1.0",
    id,
    createdAt: now.toISOString(),
    headCommit,
    files: entries,
  };
  const directory = recoveryDir(root);
  mkdirSync(directory, { recursive: true });
  const snapshotPath = join(directory, `${id}.json`);
  writeFileSync(snapshotPath, `${stableStringify(snapshot, 2)}\n`, "utf8");
  return {
    snapshotPath: relative(root, snapshotPath).replaceAll("\\", "/"),
    restoreCommand: `shrike recover ${relative(root, snapshotPath).replaceAll("\\", "/")}`,
  };
}

function readSnapshot(root: string, snapshotPath: string): RecoverySnapshot {
  const full = isAbsolute(snapshotPath)
    ? resolveInside(root, relative(root, snapshotPath))
    : resolveInside(root, snapshotPath);
  const parsed: unknown = JSON.parse(readFileSync(full, "utf8"));
  if (typeof parsed !== "object" || parsed === null) {
    throw new RecoveryError(`recovery snapshot is not an object: ${snapshotPath}`, snapshotPath);
  }
  const record = parsed as Record<string, unknown>;
  if (record.schemaVersion !== "1.0" || !Array.isArray(record.files)) {
    throw new RecoveryError(`unsupported recovery snapshot: ${snapshotPath}`, snapshotPath);
  }
  const files: RecoveryFile[] = [];
  for (const item of record.files) {
    if (
      typeof item !== "object" ||
      item === null ||
      typeof (item as RecoveryFile).path !== "string" ||
      typeof (item as RecoveryFile).content !== "string" ||
      typeof (item as RecoveryFile).contentHash !== "string"
    ) {
      throw new RecoveryError(`malformed recovery entry: ${snapshotPath}`, snapshotPath);
    }
    const entry = item as RecoveryFile;
    if (contentHash(entry.content) !== entry.contentHash) {
      throw new RecoveryError(`recovery snapshot hash mismatch: ${entry.path}`, snapshotPath);
    }
    files.push({ path: safeRelativePath(entry.path), content: entry.content, contentHash: entry.contentHash });
  }
  return {
    schemaVersion: "1.0",
    id: typeof record.id === "string" ? record.id : "unknown",
    createdAt: typeof record.createdAt === "string" ? record.createdAt : "",
    headCommit: typeof record.headCommit === "string" ? record.headCommit : "",
    files,
  };
}

/** Restore every file in a snapshot and verify the bytes afterward. */
export function restoreRecoverySnapshot(
  root: string,
  snapshotPath: string,
): { restored: string[]; snapshot: RecoverySnapshot } {
  const snapshot = readSnapshot(root, snapshotPath);
  const restored: string[] = [];
  for (const entry of snapshot.files) {
    const full = resolveInside(root, entry.path);
    mkdirSync(dirname(full), { recursive: true });
    writeFileSync(full, entry.content, "utf8");
    const actual = readFileSync(full, "utf8");
    if (contentHash(actual) !== entry.contentHash) {
      throw new RecoveryError(
        `restore verification failed for ${entry.path}`,
        snapshotPath,
      );
    }
    restored.push(entry.path);
  }
  return { restored, snapshot };
}
