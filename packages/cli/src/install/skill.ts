/**
 * The portable Shrike skill.
 *
 * The authored source of truth is `skills/shrike/` at the repository root. The
 * build copies it into `packages/cli/dist/skills/`; this module only locates,
 * validates, and loads it. Target adapters point at the installed portable
 * skill rather than maintaining a second copy.
 */
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, isAbsolute, join, relative, resolve, sep } from "node:path";

export interface SkillFile {
  /** Path relative to the skill directory, e.g. "SKILL.md". */
  path: string;
  content: string;
}

export class SkillLoadError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "SkillLoadError";
  }
}

const SKILL_NAME = "shrike";
const REFERENCES = "references";

/**
 * Candidate directories from one module directory, in resolution order:
 * bundled `dist/skills/`, source `packages/cli/src/install/`, then an
 * unbundled `dist/install/` fallback.
 */
export function skillDirectoryCandidates(moduleDirectory: string): string[] {
  return [
    join(moduleDirectory, "skills", SKILL_NAME),
    join(moduleDirectory, "..", "..", "..", "..", "skills", SKILL_NAME),
    join(moduleDirectory, "..", "..", "skills", SKILL_NAME),
  ].map((path) => resolve(path));
}

function isMarkdown(path: string): boolean {
  return path.endsWith(".md");
}

function display(directory: string, path: string): string {
  return relative(directory, path).split(sep).join("/");
}

function ensureInside(directory: string, path: string): void {
  const rel = relative(directory, path);
  if (rel.length === 0) return;
  if (rel.startsWith(`..${sep}`) || rel === ".." || isAbsolute(rel)) {
    throw new SkillLoadError(`skill path escapes the skill directory: ${path}`);
  }
}

function normalize(content: string): string {
  if (content.includes("\r\n")) {
    throw new SkillLoadError("skill files must use LF line endings");
  }
  return content.endsWith("\n") ? content : `${content}\n`;
}

/**
 * Relative Markdown link targets from a file. Fenced code blocks are ignored;
 * external URLs are validated by the static check rather than the loader.
 */
export function markdownLinkTargets(content: string): string[] {
  const withoutFences = content.replace(/^```[\s\S]*?^```/gm, "");
  const targets: string[] = [];
  for (const match of withoutFences.matchAll(/\[[^\]]*\]\(([^)\s]+)(?:\s+"[^"]*")?\)/g)) {
    const target = match[1];
    if (target !== undefined) targets.push(target);
  }
  return targets;
}

function validateLinks(directory: string, path: string, content: string): void {
  for (const target of markdownLinkTargets(content)) {
    if (target.startsWith("#")) continue;
    if (/^https:/i.test(target)) continue;
    if (/^[a-z][a-z0-9+.-]*:/i.test(target)) {
      throw new SkillLoadError(`${display(directory, path)} has an unsupported link: ${target}`);
    }
    const pathPart = target.split("#")[0] ?? "";
    if (pathPart.length === 0) continue;
    const resolved = resolve(dirname(path), pathPart);
    ensureInside(directory, resolved);
    if (!existsSync(resolved) || !statSync(resolved).isFile()) {
      throw new SkillLoadError(
        `${display(directory, path)} links to a missing file: ${pathPart}`,
      );
    }
  }
}

function readMarkdown(directory: string, path: string): string {
  if (!isMarkdown(path)) {
    throw new SkillLoadError(
      `unexpected non-Markdown file in the shipped skill: ${display(directory, path)}`,
    );
  }
  return normalize(readFileSync(path, "utf8"));
}

function listReferences(directory: string): string[] {
  const references = join(directory, REFERENCES);
  if (!existsSync(references)) return [];
  if (!statSync(references).isDirectory()) {
    throw new SkillLoadError(`${REFERENCES} must be a directory in the shipped skill`);
  }
  const files: string[] = [];
  for (const entry of readdirSync(references, { withFileTypes: true })) {
    const path = join(references, entry.name);
    if (entry.isDirectory()) {
      throw new SkillLoadError(
        `nested reference directories are not supported: ${display(directory, path)}`,
      );
    }
    if (!entry.isFile()) {
      throw new SkillLoadError(
        `unexpected entry in the shipped skill: ${display(directory, path)}`,
      );
    }
    if (!isMarkdown(path)) {
      throw new SkillLoadError(
        `unexpected non-Markdown file in the shipped skill: ${display(directory, path)}`,
      );
    }
    files.push(path);
  }
  return files.sort((left, right) => (left < right ? -1 : left > right ? 1 : 0));
}

/** Load one skill directory. Order is deterministic: SKILL.md, then references. */
export function loadShrikeSkillFrom(directory: string): SkillFile[] {
  const main = join(directory, "SKILL.md");
  if (!existsSync(main)) {
    throw new SkillLoadError(`SKILL.md was not found in ${directory}`);
  }
  if (!statSync(main).isFile()) {
    throw new SkillLoadError(`SKILL.md is not a file in ${directory}`);
  }
  const mainContent = readMarkdown(directory, main);
  if (mainContent.trim().length === 0) {
    throw new SkillLoadError(`SKILL.md is empty in ${directory}`);
  }
  validateLinks(directory, main, mainContent);

  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (entry.name === "SKILL.md") continue;
    if (entry.name === REFERENCES && entry.isDirectory()) continue;
    const path = join(directory, entry.name);
    throw new SkillLoadError(
      entry.isDirectory()
        ? `unexpected directory in the shipped skill: ${display(directory, path)}`
        : !isMarkdown(path)
          ? `unexpected non-Markdown file in the shipped skill: ${display(directory, path)}`
          : `unexpected file in the shipped skill: ${display(directory, path)}`,
    );
  }

  const files: SkillFile[] = [{ path: "SKILL.md", content: mainContent }];
  for (const path of listReferences(directory)) {
    const content = readMarkdown(directory, path);
    validateLinks(directory, path, content);
    files.push({ path: display(directory, path), content });
  }
  return files;
}

/** The first candidate with a non-empty SKILL.md, or null when none exists. */
export function resolveShrikeSkillDirectory(
  candidates: readonly string[] = skillDirectoryCandidates(import.meta.dirname),
): string | null {
  for (const directory of candidates) {
    const main = join(directory, "SKILL.md");
    try {
      if (existsSync(main) && statSync(main).isFile() && readFileSync(main, "utf8").trim().length > 0) {
        return directory;
      }
    } catch {
      // An unreadable candidate is not valid; try the next layout.
    }
  }
  return null;
}

/** Load the portable skill from the packaged or source layout. */
export function loadShrikeSkill(): SkillFile[] {
  const directory = resolveShrikeSkillDirectory();
  if (directory === null) {
    throw new SkillLoadError(
      "the Shrike skill files were not found; reinstall the CLI or run `pnpm run build:ts`",
    );
  }
  return loadShrikeSkillFrom(directory);
}

export function claudeSkill(): string {
  return `---
name: shrike
description: Find, explain, and remove dead code in TypeScript, JavaScript, React, and Next.js repositories with Shrike's whole-graph analysis and verified deletion batches. Use when the user asks what is dead, wants a branch reviewed, or wants a cleanup verified.
license: MIT
---
# Shrike

This project uses Shrike. Read and follow the portable skill and its references
at \`.agents/skills/shrike/SKILL.md\` as needed. The skill carries the workflow,
safety rules, and exact commands.
`;
}

export function cursorRule(): string {
  return `---
description: Shrike verified dead-code cleanup
globs:
alwaysApply: false
---
# Shrike

Use Shrike when the user asks what is dead or wants unused code removed. Read
\`.agents/skills/shrike/SKILL.md\` and its references for the workflow, safety
rules, and exact commands.
`;
}

export function geminiSkill(): string {
  return `---
name: shrike
description: Find, explain, and remove dead code in TypeScript, JavaScript, React, and Next.js repositories with Shrike's whole-graph analysis and verified deletion batches. Use when the user asks what is dead, wants a branch reviewed, or wants a cleanup verified.
license: MIT
---
# Shrike

Read and follow the portable skill and its references at
\`.agents/skills/shrike/SKILL.md\` as needed. The skill carries the workflow,
safety rules, and exact commands.
`;
}

export function opencodeManagedBlock(): string {
  return `// shrike:begin
// Shrike: verified dead-code cleanup. Read .agents/skills/shrike/SKILL.md and
// its references for the workflow, safety rules, and exact commands.
// shrike:end`;
}

export function agentsManagedBlock(): string {
  return `<!-- shrike:begin -->
## Shrike cleanup

This repository uses Shrike for verified dead-code cleanup. Read
\`.agents/skills/shrike/SKILL.md\` and its references for the workflow, safety
rules, and exact commands.
<!-- shrike:end -->`;
}

export function geminiManagedBlock(): string {
  return `<!-- shrike:begin -->
## Shrike cleanup

Read \`.agents/skills/shrike/SKILL.md\` and its references for the workflow,
safety rules, and exact commands.
<!-- shrike:end -->`;
}

export function copilotManagedBlock(): string {
  return `<!-- shrike:begin -->
## Shrike cleanup

When asked to find or remove dead code, read \`.agents/skills/shrike/SKILL.md\`
and its references for the workflow, safety rules, and exact commands.
<!-- shrike:end -->`;
}
