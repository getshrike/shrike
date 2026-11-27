/** Supported agent hook installers. */
import { existsSync, mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";

export type HookTarget = "claude" | "cursor" | "opencode";

export interface InstallResult {
  /** Primary configuration path that was changed. */
  path: string;
  changed: boolean;
  /** Every file touched or considered. */
  files: string[];
}

export class HookConfigError extends Error {
  readonly path: string;

  constructor(path: string, message: string) {
    super(`${message}: ${path}`);
    this.name = "HookConfigError";
    this.path = path;
  }
}

export const HOOK_COMMAND = "npx getshrike hook run --quiet";
export const OPENCODE_PLUGIN_MARKER = "// shrike-hook v1";

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function canonicalize(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(canonicalize);
  if (isRecord(value)) {
    const result: Record<string, unknown> = {};
    for (const key of Object.keys(value).sort()) {
      result[key] = canonicalize(value[key]);
    }
    return result;
  }
  return value;
}

export function stable(value: unknown, space = 0): string {
  return JSON.stringify(canonicalize(value), null, space);
}

export function readJsonObject(path: string): Record<string, unknown> {
  if (!existsSync(path)) return {};
  let parsed: unknown;
  try {
    parsed = JSON.parse(readFileSync(path, "utf8"));
  } catch {
    throw new HookConfigError(path, "refusing to overwrite invalid JSON");
  }
  if (!isRecord(parsed)) {
    throw new HookConfigError(path, "refusing to overwrite a non-object config");
  }
  return parsed;
}

export function writeFileAtomic(path: string, content: string): void {
  mkdirSync(dirname(path), { recursive: true });
  const temporary = `${path}.${process.pid}.tmp`;
  writeFileSync(temporary, content, "utf8");
  renameSync(temporary, path);
}

export function writeJsonIfChanged(
  path: string,
  value: Record<string, unknown>,
  before: Record<string, unknown> | null,
): boolean {
  if (before !== null && stable(before) === stable(value)) return false;
  writeFileAtomic(path, `${stable(value, 2)}\n`);
  return true;
}

export function isShrikeCommand(value: unknown): boolean {
  return typeof value === "string" && value.includes("shrike hook run");
}

export function isShrikeHookEntry(value: unknown): boolean {
  if (!isRecord(value)) return false;
  if (isShrikeCommand(value.command)) return true;
  if (Array.isArray(value.hooks)) {
    return value.hooks.some((hook) => isRecord(hook) && isShrikeCommand(hook.command));
  }
  return false;
}
