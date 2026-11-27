import { existsSync } from "node:fs";

import { booleanFlag, hasFlag, rejectUnknownFlags, stringFlags, type ParsedArgs } from "../args.ts";
import { ExitCode, UsageError } from "../errors.ts";
import { writeHuman, writeJson } from "../output.ts";
import {
  installClaudeHook,
  installCursorHook,
  installOpenCodeHook,
  type HookTarget,
} from "@shrike/agent";
import {
  detectTargets,
  parseTarget,
  type InstallTarget,
} from "../install/detect.ts";
import {
  addPackageScripts,
  compareWholeFile,
  readInstallFile,
  safeInstallPath,
  upsertManagedBlock,
  writeInstallFile,
  type FileAction,
  type ScriptChange,
} from "../install/files.ts";
import {
  agentsManagedBlock,
  claudeSkill,
  copilotManagedBlock,
  cursorRule,
  geminiManagedBlock,
  geminiSkill,
  loadShrikeSkill,
  opencodeManagedBlock,
  type SkillFile,
} from "../install/skill.ts";

const INSTALL_FLAGS = [
  "json",
  "help",
  "force",
  "dry-run",
  "target",
  "claude",
  "cursor",
  "opencode",
  "no-skill",
  "no-hook",
  "no-scripts",
  "scripts",
];

interface HookAction {
  target: HookTarget;
  action: "installed" | "unchanged" | "failed" | "unsupported";
  detail?: string;
}

interface InstallResultJson {
  targets: InstallTarget[];
  actions: FileAction[];
  hook: HookAction | null;
  hooks: HookAction[];
  scripts: ScriptChange[];
  next: string;
  dryRun?: true;
}

function sanitize(message: string, root: string): string {
  return message.replaceAll(root, "").replace(/\/{2,}/g, "/").replace(/^: /, "").trim() || "install failed";
}

function sortedTargets(root: string, args: ParsedArgs): InstallTarget[] {
  const explicit: InstallTarget[] = [];
  if (hasFlag(args, "target")) {
    const values = stringFlags(args, "target");
    if (values.length === 0) throw new UsageError("--target requires a value");
    for (const value of values) {
      try {
        explicit.push(...parseTarget(value));
      } catch (error) {
        throw new UsageError(error instanceof Error ? error.message : String(error));
      }
    }
  }
  for (const [flag, target] of [
    ["claude", "claude"],
    ["cursor", "cursor"],
    ["opencode", "opencode"],
  ] as const) {
    if (booleanFlag(args, flag)) explicit.push(target);
  }
  const selected = explicit.length > 0 ? explicit : detectTargets(root);
  if (selected.length === 0) selected.push("agents");
  return [...new Set(selected)].sort();
}

function writeWhole(
  root: string,
  path: string,
  content: string,
  force: boolean,
  dryRun: boolean,
): FileAction {
  try {
    const existing = readInstallFile(root, path);
    const planned = compareWholeFile(existing, content, force);
    if (!dryRun && (planned.action === "created" || planned.action === "updated")) {
      writeInstallFile(root, path, planned.content);
    }
    return planned.detail
      ? { path, action: planned.action, detail: planned.detail }
      : { path, action: planned.action };
  } catch (error) {
    return { path, action: "failed", detail: sanitize(error instanceof Error ? error.message : String(error), root) };
  }
}

const SKILL_ROOT = ".agents/skills/shrike";

/**
 * Install the whole skill directory as one unit.
 *
 * Conflict handling is all-or-nothing: if any existing file differs and
 * `--force` is not set, no skill file is written. A new `SKILL.md` must never
 * point at a reference from an older installed version.
 */
export function installSkillDirectory(
  root: string,
  files: readonly SkillFile[],
  force: boolean,
  dryRun: boolean,
): FileAction[] {
  const planned: { path: string; content: string; existing: string | null }[] = [];
  try {
    for (const file of files) {
      const path = `${SKILL_ROOT}/${file.path}`;
      planned.push({ path, content: file.content, existing: readInstallFile(root, path) });
    }
  } catch (error) {
    return [{
      path: SKILL_ROOT,
      action: "failed",
      detail: sanitize(error instanceof Error ? error.message : String(error), root),
    }];
  }

  const changed = planned.filter((entry) => entry.existing !== null && entry.existing !== entry.content);
  if (changed.length > 0 && !force) {
    return planned.map((entry) => {
      if (entry.existing === entry.content) return { path: entry.path, action: "unchanged" as const };
      if (entry.existing !== null) {
        return {
          path: entry.path,
          action: "skipped" as const,
          detail: "existing file has local edits; rerun with --force to replace it",
        };
      }
      return {
        path: entry.path,
        action: "skipped" as const,
        detail: "skill directory left unchanged to avoid a mixed version; rerun with --force",
      };
    });
  }

  const actions: FileAction[] = [];
  for (const entry of planned) {
    const action = entry.existing === null
      ? "created"
      : entry.existing === entry.content ? "unchanged" : "updated";
    if (!dryRun && (action === "created" || action === "updated")) {
      try {
        writeInstallFile(root, entry.path, entry.content);
      } catch (error) {
        actions.push({
          path: entry.path,
          action: "failed",
          detail: sanitize(error instanceof Error ? error.message : String(error), root),
        });
        continue;
      }
    }
    actions.push({ path: entry.path, action });
  }
  return actions;
}

function writeManaged(
  root: string,
  path: string,
  block: string,
  style: "markdown" | "jsonc",
  dryRun: boolean,
): FileAction {
  try {
    const existing = readInstallFile(root, path);
    if (existing === null) {
      return { path, action: "skipped", detail: "file is not present; add the snippet manually" };
    }
    const planned = upsertManagedBlock(existing, block, style);
    if (!dryRun && (planned.action === "created" || planned.action === "updated")) {
      writeInstallFile(root, path, planned.content);
    }
    return planned.detail
      ? { path, action: planned.action, detail: planned.detail }
      : { path, action: planned.action };
  } catch (error) {
    return { path, action: "failed", detail: sanitize(error instanceof Error ? error.message : String(error), root) };
  }
}

function installHooks(root: string, targets: InstallTarget[], dryRun: boolean): HookAction[] {
  const actions: HookAction[] = [];
  for (const target of targets) {
    if (target !== "claude" && target !== "cursor" && target !== "opencode") continue;
    if (dryRun) {
      actions.push({ target, action: "installed" });
      continue;
    }
    try {
      safeInstallPath(root, target === "claude"
        ? ".claude/settings.json"
        : target === "cursor"
          ? ".cursor/hooks.json"
          : "opencode.json");
      const result = target === "claude"
        ? installClaudeHook(root)
        : target === "cursor"
          ? installCursorHook(root)
          : installOpenCodeHook(root);
      actions.push({ target, action: result.changed ? "installed" : "unchanged" });
    } catch (error) {
      actions.push({
        target,
        action: "failed",
        detail: sanitize(error instanceof Error ? error.message : String(error), root),
      });
    }
  }
  return actions.sort((left, right) => (left.target < right.target ? -1 : left.target > right.target ? 1 : 0));
}

function installScripts(root: string, dryRun: boolean): ScriptChange[] {
  const path = "package.json";
  try {
    const existing = readInstallFile(root, path);
    if (existing === null) {
      return [
        { name: "shrike:report", action: "skipped" },
        { name: "shrike:clean", action: "skipped" },
        { name: "shrike:ci", action: "skipped" },
      ];
    }
    const result = addPackageScripts(existing, {
      "shrike:report": "shrike report",
      "shrike:clean": "shrike clean",
      "shrike:ci": "shrike ci install",
    });
    if (!dryRun && result.content !== null && result.content !== existing) {
      writeInstallFile(root, path, result.content);
    }
    return result.changes.sort((left, right) => (left.name < right.name ? -1 : left.name > right.name ? 1 : 0));
  } catch (error) {
    void error;
    return [{ name: "package.json", action: "failed" }];
  }
}

export interface InstallOptions {
  /** Test seam for the canonical skill loader; production uses loadShrikeSkill. */
  loadSkill?: () => SkillFile[];
}

export async function installCommand(
  root: string,
  args: ParsedArgs,
  options: InstallOptions = {},
): Promise<number> {
  rejectUnknownFlags(args, INSTALL_FLAGS);
  if (args.positionals.length > 0) {
    throw new UsageError(`unexpected argument: ${args.positionals[0]}`);
  }
  if (hasFlag(args, "scripts") && booleanFlag(args, "no-scripts")) {
    throw new UsageError("--scripts and --no-scripts cannot be used together");
  }
  const dryRun = booleanFlag(args, "dry-run");
  const force = booleanFlag(args, "force");
  const targets = sortedTargets(root, args);
  const actions: FileAction[] = [];

  if (!booleanFlag(args, "no-skill")) {
    let skillFiles: SkillFile[] | null = null;
    try {
      skillFiles = (options.loadSkill ?? loadShrikeSkill)();
    } catch (error) {
      actions.push({
        path: SKILL_ROOT,
        action: "failed",
        detail: sanitize(error instanceof Error ? error.message : String(error), root),
      });
    }
    if (skillFiles !== null) {
      actions.push(...installSkillDirectory(root, skillFiles, force, dryRun));
      for (const target of targets) {
        if (target === "claude") {
          actions.push(writeWhole(root, ".claude/skills/shrike/SKILL.md", claudeSkill(), force, dryRun));
          if (existsSync(`${root}/CLAUDE.md`)) {
            actions.push(writeManaged(root, "CLAUDE.md", agentsManagedBlock(), "markdown", dryRun));
          }
        } else if (target === "cursor") {
          actions.push(writeWhole(root, ".cursor/rules/shrike.mdc", cursorRule(), force, dryRun));
          if (existsSync(`${root}/.cursorrules`)) {
            actions.push(writeManaged(root, ".cursorrules", agentsManagedBlock(), "markdown", dryRun));
          }
        } else if (target === "opencode") {
          if (existsSync(`${root}/opencode.jsonc`)) {
            actions.push(writeManaged(root, "opencode.jsonc", opencodeManagedBlock(), "jsonc", dryRun));
          } else {
            actions.push({
              path: "opencode.jsonc",
              action: "skipped",
              detail: "no comment-capable opencode.jsonc; add the snippet manually",
            });
          }
        } else if (target === "codex") {
          if (existsSync(`${root}/AGENTS.md`)) {
            actions.push(writeManaged(root, "AGENTS.md", agentsManagedBlock(), "markdown", dryRun));
          } else {
            actions.push({
              path: "AGENTS.md",
              action: "skipped",
              detail: "AGENTS.md is not present; add the snippet manually",
            });
          }
        } else if (target === "gemini") {
          actions.push(writeWhole(root, ".gemini/skills/shrike/SKILL.md", geminiSkill(), force, dryRun));
          if (existsSync(`${root}/GEMINI.md`)) {
            actions.push(writeManaged(root, "GEMINI.md", geminiManagedBlock(), "markdown", dryRun));
          }
        } else if (target === "copilot") {
          if (existsSync(`${root}/.github/copilot-instructions.md`)) {
            actions.push(writeManaged(root, ".github/copilot-instructions.md", copilotManagedBlock(), "markdown", dryRun));
          } else {
            actions.push(writeWhole(root, ".github/copilot-instructions.md", `${copilotManagedBlock()}\n`, force, dryRun));
          }
        }
      }
    }
  }

  const hookTargets = targets.filter((target): target is HookTarget =>
    target === "claude" || target === "cursor" || target === "opencode",
  );
  const hooks = booleanFlag(args, "no-hook") ? [] : installHooks(root, hookTargets, dryRun);

  const scripts = booleanFlag(args, "scripts") && !booleanFlag(args, "no-scripts")
    ? installScripts(root, dryRun)
    : [];

  actions.sort((left, right) => (left.path < right.path ? -1 : left.path > right.path ? 1 : 0));

  const result: InstallResultJson = {
    targets,
    actions,
    hook: hooks[0] ?? null,
    hooks,
    scripts,
    next: "npx getshrike report",
    ...(dryRun ? { dryRun: true as const } : {}),
  };

  if (booleanFlag(args, "json")) {
    writeJson(result);
  } else {
    writeHuman(`Shrike install${dryRun ? " (dry run: no files written)" : ""}`);
    for (const action of actions) {
      const detail = action.detail ? ` (${action.detail})` : "";
      writeHuman(`  ${action.action.padEnd(9)} ${action.path}${detail}`);
    }
    for (const hook of hooks) {
      const detail = hook.detail ? ` (${hook.detail})` : "";
      writeHuman(`  hook      ${hook.target} (${hook.action})${detail}`);
    }
    if (scripts.length > 0) {
      writeHuman(`  scripts   ${scripts.map((entry) => `${entry.name} (${entry.action})`).join(", ")}`);
    } else {
      writeHuman("  scripts   none");
    }
    writeHuman("");
    writeHuman(`Next: ${result.next}`);
  }

  const failed = actions.some((action) => action.action === "failed")
    || hooks.some((hook) => hook.action === "failed")
    || scripts.some((entry) => entry.action === "failed");
  return failed ? ExitCode.COULD_NOT_RUN : ExitCode.OK;
}
