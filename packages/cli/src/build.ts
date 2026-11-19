import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { isAbsolute, join, relative, resolve } from "node:path";
import { resolveInside, safeRelativePath } from "./recovery.ts";

/**
 * Build-graph evidence loading.
 *
 * Next.js 16.4 writes `next analyze export` as JSON Lines. Older or hand-made
 * evidence is often one JSON object. Both are normalized into the small shape
 * understood by the Rust `join-build` command. The parser is deliberately
 * conservative: a malformed record is an explicit error, never a silent skip.
 */

export type BuildStatus =
  | "full"
  | "partial"
  | "low-coverage"
  | "stale"
  | "malformed"
  | "unsupported"
  | "absent"
  | "build-failed";

export interface BuildRouteEvidence {
  route: string;
  files: string[];
  symbols: string[];
}

export interface BuildEvidence {
  source: string;
  version: string;
  files: string[];
  symbols: string[];
  routes: BuildRouteEvidence[];
  sizes: Record<string, number>;
  snapshotHash: string;
}

export interface LoadedBuildEvidence {
  evidence: BuildEvidence;
  path: string | null;
  /** Set when the file exists but could not be normalized. */
  error: string | null;
}

export class BuildEvidenceError extends Error {
  readonly status: BuildStatus;

  constructor(status: BuildStatus, message: string) {
    super(message);
    this.name = "BuildEvidenceError";
    this.status = status;
  }
}

export const EMPTY_BUILD_HASH = `sha256:${createHash("sha256").update("").digest("hex")}`;

function sha256(value: string): string {
  return `sha256:${createHash("sha256").update(value, "utf8").digest("hex")}`;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function stringArray(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  return value.filter((item): item is string => typeof item === "string" && item.length > 0);
}

function numberValue(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) && value >= 0 ? value : null;
}

function normalizePath(root: string, input: string): string {
  let value = input.replaceAll("\\", "/").trim();
  const loader = value.lastIndexOf("!");
  if (loader >= 0) value = value.slice(loader + 1);
  value = value.replace(/^turbopack:\/\//, "").replace(/^file:\/\//, "");
  value = value.split("?")[0]!.split("#")[0]!;
  if (isAbsolute(value)) value = relative(root, value).replaceAll("\\", "/");
  value = value.replace(/^\.\//, "");
  return value;
}

function addUnique(list: string[], value: string): void {
  if (value.length > 0 && !list.includes(value)) list.push(value);
}

/** Read the installed Next.js version from package.json, if present. */
export function packageNextVersion(root: string): string {
  const path = join(root, "package.json");
  if (!existsSync(path)) return "";
  try {
    const parsed: unknown = JSON.parse(readFileSync(path, "utf8"));
    if (!isRecord(parsed)) return "";
    const dependencies = isRecord(parsed.dependencies) ? parsed.dependencies : {};
    const devDependencies = isRecord(parsed.devDependencies) ? parsed.devDependencies : {};
    const value = dependencies.next ?? devDependencies.next;
    return typeof value === "string" ? value.replace(/^[~^]/, "") : "";
  } catch {
    return "";
  }
}

function lowerVersion(left: string, right: string): string {
  if (!left) return right;
  if (!right) return left;
  const parse = (value: string): number[] =>
    value.replace(/^[~^v]/, "").split(".").map((part) => Number.parseInt(part, 10) || 0);
  const a = parse(left);
  const b = parse(right);
  for (let index = 0; index < Math.max(a.length, b.length); index += 1) {
    const delta = (a[index] ?? 0) - (b[index] ?? 0);
    if (delta !== 0) return delta < 0 ? left : right;
  }
  return left;
}

/** Find the newest default evidence file without running a build. */
export function discoverBuildEvidence(root: string): string | null {
  const directories = [
    join(root, ".next", "analyze"),
    join(root, ".shrike", "build"),
    join(root, "next-analyze"),
  ];
  const candidates: string[] = [];
  for (const directory of directories) {
    if (!existsSync(directory)) continue;
    for (const name of readdirSync(directory)) {
      if (!name.endsWith(".json") && !name.endsWith(".jsonl")) continue;
      const path = join(directory, name);
      try {
        if (statSync(path).isFile()) candidates.push(path);
      } catch {
        // A disappearing file is not evidence.
      }
    }
  }
  candidates.sort((left, right) => {
    const leftTime = statSync(left).mtimeMs;
    const rightTime = statSync(right).mtimeMs;
    return rightTime - leftTime || right.localeCompare(left);
  });
  return candidates[0] ?? null;
}

function recordFile(record: Record<string, unknown>): string | null {
  for (const key of ["path", "file", "source", "module"]) {
    const value = record[key];
    if (typeof value === "string" && value.length > 0) return value;
  }
  return null;
}

function recordSymbol(record: Record<string, unknown>): string | null {
  for (const key of ["symbol", "symbolKey", "key"]) {
    const value = record[key];
    if (typeof value === "string" && value.includes("#")) return value;
  }
  return null;
}

function recordRoute(record: Record<string, unknown>): string | null {
  for (const key of ["route", "pathname", "entry"]) {
    const value = record[key];
    if (typeof value === "string" && value.length > 0) return value;
  }
  return null;
}

function recordSize(record: Record<string, unknown>): number | null {
  for (const key of ["size", "bytes", "byteSize"]) {
    const value = numberValue(record[key]);
    if (value !== null) return value;
  }
  return null;
}

function recordPaths(record: Record<string, unknown>, keys: string[]): string[] {
  const result: string[] = [];
  for (const key of keys) {
    for (const value of stringArray(record[key])) result.push(value);
  }
  return result;
}

/**
 * Normalize one JSON object or a JSONL record stream. Unknown record families
 * are ignored; malformed JSON is not.
 */
export function normalizeBuildEvidence(
  root: string,
  text: string,
  sourcePath: string,
): BuildEvidence {
  const files: string[] = [];
  const symbols: string[] = [];
  const routes: BuildRouteEvidence[] = [];
  const sizes: Record<string, number> = {};
  let source = "next analyze export";
  let version = packageNextVersion(root);
  let snapshotHash = sha256(text);

  const consume = (record: Record<string, unknown>, line?: number): void => {
    const type = String(record.type ?? record.kind ?? record.record ?? "").toLowerCase();
    const where = line === undefined ? sourcePath : `${sourcePath}:${line}`;
    if (type === "module") {
      const path = recordFile(record);
      if (!path) throw new BuildEvidenceError("malformed", `module record has no path at ${where}`);
      const normalized = normalizePath(root, path);
      addUnique(files, normalized);
      const size = recordSize(record);
      if (size !== null) sizes[normalized] = size;
      return;
    }
    if (type === "chunk") {
      const route = recordRoute(record);
      const chunkFiles = recordPaths(record, ["files", "modules", "moduleIds"]).map((path) => normalizePath(root, path));
      const chunkSymbols = recordPaths(record, ["symbols", "symbolKeys"]);
      const size = recordSize(record);
      if (route) {
        const existing = routes.find((item) => item.route === route);
        const target = existing ?? { route, files: [], symbols: [] };
        for (const file of chunkFiles) addUnique(target.files, file);
        for (const symbol of chunkSymbols) addUnique(target.symbols, symbol);
        if (!existing) routes.push(target);
      }
      for (const file of chunkFiles) addUnique(files, file);
      for (const symbol of chunkSymbols) addUnique(symbols, symbol);
      if (size !== null && route) {
        // A chunk size is evidence for the route, not a module size.
        for (const file of chunkFiles) if (sizes[file] === undefined) sizes[file] = size;
      }
      return;
    }
    if (type === "entry" || type === "route") {
      const route = recordRoute(record);
      if (!route) return;
      const entryFiles = recordPaths(record, ["files", "modules", "moduleIds"]).map((path) => normalizePath(root, path));
      const entrySymbols = recordPaths(record, ["symbols", "symbolKeys"]);
      const existing = routes.find((item) => item.route === route);
      const target = existing ?? { route, files: [], symbols: [] };
      for (const file of entryFiles) addUnique(target.files, file);
      for (const symbol of entrySymbols) addUnique(target.symbols, symbol);
      if (!existing) routes.push(target);
      for (const file of entryFiles) addUnique(files, file);
      for (const symbol of entrySymbols) addUnique(symbols, symbol);
      return;
    }
    if (type === "client-reference" || type === "clientreference") {
      const file = recordFile(record);
      if (file) addUnique(files, normalizePath(root, file));
      const symbol = recordSymbol(record);
      if (symbol) addUnique(symbols, symbol);
      return;
    }
    // Asset and unknown record families are additive and ignored.
  };

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
      if (index === 0 && !String(record.type ?? record.kind ?? record.record ?? "").length) {
        // A plain evidence object (the documented simple shape).
        if (typeof record.source === "string" && record.source.length > 0) source = record.source;
        if (typeof record.version === "string" && record.version.length > 0) version = lowerVersion(version, record.version);
        if (typeof record.snapshotHash === "string" && record.snapshotHash.length > 0) snapshotHash = record.snapshotHash;
        for (const file of stringArray(record.files)) addUnique(files, normalizePath(root, file));
        for (const symbol of stringArray(record.symbols)) addUnique(symbols, symbol);
        if (isRecord(record.sizes)) {
          for (const [key, value] of Object.entries(record.sizes)) {
            const size = numberValue(value);
            if (size !== null) sizes[normalizePath(root, key)] = size;
          }
        }
        if (Array.isArray(record.routes)) {
          for (const item of record.routes) {
            if (!isRecord(item)) continue;
            const route = recordRoute(item);
            if (!route) continue;
            routes.push({
              route,
              files: stringArray(item.files).map((file) => normalizePath(root, file)),
              symbols: stringArray(item.symbols),
            });
          }
        }
        continue;
      }
      consume(record);
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
      consume(record, index + 1);
    }
  }

  // Route files/symbols are part of the evidence even when the top-level list
  // omitted them; the engine deduplicates.
  return {
    source,
    version,
    files,
    symbols,
    routes,
    sizes,
    snapshotHash,
  };
}

function emptyEvidence(root: string): BuildEvidence {
  return {
    source: "static",
    version: packageNextVersion(root) || "unknown",
    files: [],
    symbols: [],
    routes: [],
    sizes: {},
    snapshotHash: EMPTY_BUILD_HASH,
  };
}

/** Load explicit evidence or discover a default snapshot. */
export function loadBuildEvidence(root: string, explicitPath?: string | null): LoadedBuildEvidence {
  let path: string | null;
  try {
    path = explicitPath ? resolveEvidencePath(root, explicitPath) : discoverBuildEvidence(root);
  } catch (error) {
    if (error instanceof BuildEvidenceError) {
      return { evidence: emptyEvidence(root), path: null, error: error.message };
    }
    throw error;
  }
  if (!path) {
    return { evidence: emptyEvidence(root), path: null, error: null };
  }
  try {
    const text = readFileSync(path, "utf8");
    return { evidence: normalizeBuildEvidence(root, text, path), path, error: null };
  } catch (error) {
    if (error instanceof BuildEvidenceError) {
      return { evidence: emptyEvidence(root), path, error: error.message };
    }
    throw error;
  }
}

function resolveEvidencePath(root: string, input: string): string {
  if (isAbsolute(input)) {
    const normalized = resolve(input);
    const base = resolve(root);
    if (normalized !== base && !normalized.startsWith(`${base}/`)) {
      throw new BuildEvidenceError("malformed", `build evidence is outside the repository: ${input}`);
    }
    return normalized;
  }
  try {
    return resolveInside(root, safeRelativePath(input));
  } catch (error) {
    throw new BuildEvidenceError(
      "malformed",
      error instanceof Error ? error.message : String(error),
    );
  }
}
