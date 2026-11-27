import {
  existsSync,
  lstatSync,
  mkdirSync,
  readFileSync,
  realpathSync,
  renameSync,
  writeFileSync,
} from "node:fs";
import { dirname, isAbsolute, relative, resolve, sep } from "node:path";

import { resolveInside } from "../recovery.ts";

export type ActionName = "created" | "updated" | "unchanged" | "skipped" | "failed";

export interface FileAction {
  path: string;
  action: ActionName;
  detail?: string;
}

function inside(root: string, target: string): boolean {
  const prefix = root.endsWith(sep) ? root : `${root}${sep}`;
  return target === root || target.startsWith(prefix);
}

/**
 * Resolve a repository-relative path and refuse a symlink that escapes the
 * repository. Existing parent directories are checked because a safe-looking
 * child can still be written through an escaping symlinked directory.
 */
export function safeInstallPath(root: string, path: string): string {
  const target = resolveInside(root, path);
  const realRoot = realpathSync(root);
  let current = root;
  for (const part of relative(root, target).split(sep).filter(Boolean)) {
    current = resolve(current, part);
    if (!existsSync(current)) continue;
    const stat = lstatSync(current);
    if (!stat.isSymbolicLink()) continue;
    const real = realpathSync(current);
    if (!inside(realRoot, real)) {
      throw new Error(`refusing to follow a symlink outside the repository: ${path}`);
    }
  }
  return target;
}

export function writeInstallFile(root: string, path: string, content: string): void {
  const target = safeInstallPath(root, path);
  mkdirSync(dirname(target), { recursive: true });
  const temporary = `${target}.${process.pid}.tmp`;
  writeFileSync(temporary, content, "utf8");
  renameSync(temporary, target);
}

export function readInstallFile(root: string, path: string): string | null {
  const target = safeInstallPath(root, path);
  return existsSync(target) ? readFileSync(target, "utf8") : null;
}

export interface WholeFileWrite {
  content: string;
  action: ActionName;
  detail?: string;
}

/** Compare a whole Shrike-managed file and apply the force rule. */
export function compareWholeFile(
  existing: string | null,
  content: string,
  force: boolean,
): WholeFileWrite {
  if (existing === null) return { content, action: "created" };
  if (existing === content) return { content, action: "unchanged" };
  if (force) return { content, action: "updated" };
  return {
    content: existing,
    action: "skipped",
    detail: "existing file has local edits; rerun with --force to replace it",
  };
}

export function ensureTrailingNewline(content: string): string {
  return content.endsWith("\n") ? content : `${content}\n`;
}

export interface ManagedBlockResult {
  content: string;
  action: ActionName;
  detail?: string;
}

interface MarkerStyle {
  begin: string;
  end: string;
}

const MARKDOWN: MarkerStyle = { begin: "<!-- shrike:begin -->", end: "<!-- shrike:end -->" };
const JSONC: MarkerStyle = { begin: "// shrike:begin", end: "// shrike:end" };

/**
 * Insert or replace a marker-owned block. Content outside the markers is
 * preserved byte for byte; a missing block is appended.
 */
export function upsertManagedBlock(
  existing: string | null,
  block: string,
  style: "markdown" | "jsonc" = "markdown",
): ManagedBlockResult {
  const markers = style === "markdown" ? MARKDOWN : JSONC;
  const source = existing ?? "";
  const begin = source.indexOf(markers.begin);
  const end = source.indexOf(markers.end);

  if ((begin >= 0) !== (end >= 0) || (begin >= 0 && end < begin)) {
    return {
      content: source,
      action: "skipped",
      detail: "incomplete shrike markers; repair the block manually",
    };
  }

  const normalized = ensureTrailingNewline(block);
  if (begin >= 0 && end >= 0) {
    const after = end + markers.end.length;
    const content = `${source.slice(0, begin)}${normalized.trimEnd()}${source.slice(after)}`;
    return {
      content: ensureTrailingNewline(content),
      action: content === source ? "unchanged" : "updated",
    };
  }

  const separator = source.length === 0 || source.endsWith("\n\n") ? "" : source.endsWith("\n") ? "\n" : "\n\n";
  const content = ensureTrailingNewline(`${source}${separator}${normalized}`);
  return { content, action: source.length === 0 ? "created" : "updated" };
}

/** Write a managed JSON script set, preserving indentation and final newline. */
export interface ScriptChange {
  name: string;
  action: ActionName;
}

export interface ScriptResult {
  content: string | null;
  changes: ScriptChange[];
}

export function addPackageScripts(
  text: string | null,
  additions: Record<string, string>,
): ScriptResult {
  if (text === null) {
    return { content: null, changes: Object.keys(additions).map((name) => ({ name, action: "skipped" as const })) };
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    return { content: null, changes: Object.keys(additions).map((name) => ({ name, action: "failed" as const })) };
  }
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
    return { content: null, changes: Object.keys(additions).map((name) => ({ name, action: "failed" as const })) };
  }
  const manifest = parsed as Record<string, unknown>;
  const scripts = typeof manifest.scripts === "object" && manifest.scripts !== null && !Array.isArray(manifest.scripts)
    ? { ...(manifest.scripts as Record<string, unknown>) }
    : {};
  const changes: ScriptChange[] = [];
  for (const [name, command] of Object.entries(additions)) {
    const current = scripts[name];
    if (typeof current === "string") {
      changes.push({ name, action: current === command ? "unchanged" : "skipped" });
      continue;
    }
    scripts[name] = command;
    changes.push({ name, action: "created" });
  }
  if (changes.every((change) => change.action !== "created")) {
    return { content: text, changes };
  }
  manifest.scripts = scripts;
  const indentMatch = /^([ \t]+)"/m.exec(text);
  const indent = indentMatch?.[1] ?? "  ";
  const trailing = text.endsWith("\n") ? "\n" : "";
  return { content: `${JSON.stringify(manifest, null, indent)}${trailing}`, changes };
}

export function isInsideWorkflowDirectory(path: string): boolean {
  return path.startsWith(".github/workflows/") && /\.(?:yml|yaml)$/.test(path);
}

export function validateWorkflowPath(path: string): string {
  if (isAbsolute(path) || path.includes("\\") || path.includes("\0")) {
    throw new Error(`workflow path must be repository-relative: ${path}`);
  }
  const clean = path.replace(/\\/g, "/");
  const segments = clean.split("/");
  if (segments.includes("..") || segments.includes(".") || segments.includes("")) {
    throw new Error(`workflow path must not contain . or ..: ${path}`);
  }
  if (!isInsideWorkflowDirectory(clean)) {
    throw new Error(`workflow path must be inside .github/workflows/ and end in .yml or .yaml: ${path}`);
  }
  return clean;
}
