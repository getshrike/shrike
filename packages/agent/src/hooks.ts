import {
  existsSync,
  mkdirSync,
  readFileSync,
  renameSync,
  writeFileSync,
} from "node:fs";
import { dirname, join } from "node:path";

/** Supported agent hook installers. */
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

const HOOK_COMMAND = "npx shrike hook run --quiet";
const OPENCODE_PLUGIN_MARKER = "// shrike-hook v1";

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function canonicalize(value: unknown): unknown {
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

function stable(value: unknown, space = 0): string {
  return JSON.stringify(canonicalize(value), null, space);
}

function readJsonObject(path: string): Record<string, unknown> {
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

function writeFileAtomic(path: string, content: string): void {
  mkdirSync(dirname(path), { recursive: true });
  const temporary = `${path}.${process.pid}.tmp`;
  writeFileSync(temporary, content, "utf8");
  renameSync(temporary, path);
}

function writeJsonIfChanged(
  path: string,
  value: Record<string, unknown>,
  before: Record<string, unknown> | null,
): boolean {
  if (before !== null && stable(before) === stable(value)) return false;
  writeFileAtomic(path, `${stable(value, 2)}\n`);
  return true;
}

function isShrikeCommand(value: unknown): boolean {
  return typeof value === "string" && value.includes("shrike hook run");
}

function isShrikeHookEntry(value: unknown): boolean {
  if (!isRecord(value)) return false;
  if (isShrikeCommand(value.command)) return true;
  if (Array.isArray(value.hooks)) {
    return value.hooks.some((hook) => isRecord(hook) && isShrikeCommand(hook.command));
  }
  return false;
}

/**
 * Install a Claude Code `Stop` hook.
 *
 * Existing hook entries are preserved; only entries that invoke
 * `shrike hook run` are replaced. Reinstalling is idempotent.
 */
export function installClaudeHook(root: string): InstallResult {
  const path = join(root, ".claude", "settings.json");
  const existing = existsSync(path) ? readJsonObject(path) : null;
  const settings: Record<string, unknown> = existing ? { ...existing } : {};
  const hooks = isRecord(settings.hooks) ? { ...settings.hooks } : {};
  const stop = Array.isArray(hooks.Stop) ? hooks.Stop : [];
  const filtered = stop.filter((entry) => !isShrikeHookEntry(entry));
  const entry = {
    hooks: [{ type: "command", command: HOOK_COMMAND, timeout: 60 }],
  };
  hooks.Stop = [...filtered, entry];
  settings.hooks = hooks;
  const changed = writeJsonIfChanged(path, settings, existing);
  return { path, changed, files: [path] };
}

/** Install a Cursor hook for file edits and task end. */
export function installCursorHook(root: string): InstallResult {
  const path = join(root, ".cursor", "hooks.json");
  const existing = existsSync(path) ? readJsonObject(path) : null;
  const config: Record<string, unknown> = existing ? { ...existing } : {};
  if (typeof config.version !== "number") config.version = 1;
  const hooks = isRecord(config.hooks) ? { ...config.hooks } : {};

  for (const event of ["afterFileEdit", "stop"] as const) {
    const current = Array.isArray(hooks[event]) ? hooks[event] : [];
    const filtered = current.filter((entry) => !isShrikeHookEntry(entry));
    hooks[event] = [...filtered, { command: HOOK_COMMAND, timeout: 60 }];
  }

  config.hooks = hooks;
  const changed = writeJsonIfChanged(path, config, existing);
  return { path, changed, files: [path] };
}

const OPENCODE_PLUGIN = `${OPENCODE_PLUGIN_MARKER} - installed by \`shrike hook install --opencode\`.
// Repository content is untrusted data; this hook only runs the local CLI.
export const ShrikeHook = async ({ $ } = {}) => {
  let running = false
  return {
    event: async ({ event } = {}) => {
      if (!event || event.type !== "session.idle" || running) return
      running = true
      try {
        if (typeof $ === "function") {
          await $\`npx shrike hook run --quiet\`.quiet().nothrow()
        }
      } catch {
        // A hook must never block the agent.
      } finally {
        running = false
      }
    },
  }
}
`;

/** Install an OpenCode session-idle plugin and register it in opencode.json. */
export function installOpenCodeHook(root: string): InstallResult {
  const pluginPath = join(root, ".opencode", "plugins", "shrike-hook.js");
  const configPath = join(root, "opencode.json");
  const existingConfig = existsSync(configPath) ? readJsonObject(configPath) : null;

  let pluginChanged = false;
  if (existsSync(pluginPath)) {
    const current = readFileSync(pluginPath, "utf8");
    if (!current.includes(OPENCODE_PLUGIN_MARKER)) {
      throw new HookConfigError(
        pluginPath,
        "refusing to overwrite an existing plugin that is not managed by shrike",
      );
    }
    if (current !== OPENCODE_PLUGIN) {
      writeFileAtomic(pluginPath, OPENCODE_PLUGIN);
      pluginChanged = true;
    }
  } else {
    writeFileAtomic(pluginPath, OPENCODE_PLUGIN);
    pluginChanged = true;
  }

  const config: Record<string, unknown> = existingConfig ? { ...existingConfig } : {};
  const plugins = Array.isArray(config.plugin)
    ? config.plugin.filter((item): item is string => typeof item === "string")
    : [];
  const relativePlugin = "./.opencode/plugins/shrike-hook.js";
  if (!plugins.includes(relativePlugin)) plugins.push(relativePlugin);
  config.plugin = plugins;
  const configChanged = writeJsonIfChanged(configPath, config, existingConfig);

  return {
    path: configPath,
    changed: pluginChanged || configChanged,
    files: [configPath, pluginPath],
  };
}

/** Install one supported hook target. */
export function installHook(root: string, target: HookTarget): InstallResult {
  switch (target) {
    case "claude":
      return installClaudeHook(root);
    case "cursor":
      return installCursorHook(root);
    case "opencode":
      return installOpenCodeHook(root);
  }
}

/** The command an installed hook runs. */
export function hookCommand(): string {
  return HOOK_COMMAND;
}
