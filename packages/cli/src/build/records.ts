/** Read the installed Next.js version from package.json, if present. */
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

import { isRecord, numberValue, stringArray } from "./paths.ts";

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

export function lowerVersion(left: string, right: string): string {
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

export function recordFile(record: Record<string, unknown>): string | null {
  for (const key of ["path", "file", "source", "module"]) {
    const value = record[key];
    if (typeof value === "string" && value.length > 0) return value;
  }
  return null;
}

export function recordModuleId(record: Record<string, unknown>): string | null {
  for (const key of ["id", "moduleId", "chunkId", "ident"]) {
    const value = record[key];
    if (typeof value === "string" && value.length > 0) return value;
    if (typeof value === "number" && Number.isFinite(value)) return String(value);
  }
  return null;
}

export function recordSymbol(record: Record<string, unknown>): string | null {
  for (const key of ["symbol", "symbolKey", "key"]) {
    const value = record[key];
    if (typeof value === "string" && value.includes("#")) return value;
  }
  return null;
}

export function recordRoute(record: Record<string, unknown>): string | null {
  for (const key of ["route", "pathname", "entry"]) {
    const value = record[key];
    if (typeof value === "string" && value.length > 0) return value;
  }
  return null;
}

export function recordSize(record: Record<string, unknown>): number | null {
  for (const key of ["size", "bytes", "byteSize"]) {
    const value = numberValue(record[key]);
    if (value !== null) return value;
  }
  return null;
}

export function recordPaths(record: Record<string, unknown>, keys: string[]): string[] {
  const result: string[] = [];
  for (const key of keys) {
    for (const value of stringArray(record[key])) result.push(value);
  }
  return result;
}
