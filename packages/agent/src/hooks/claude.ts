import { existsSync } from "node:fs";
import { join } from "node:path";

import {
  HOOK_COMMAND, type InstallResult, isRecord, isShrikeHookEntry, readJsonObject,
  writeJsonIfChanged,
} from "./common.ts";

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

/** Remove only Shrike-managed entries from a Claude Code settings file. */
export function uninstallClaudeHook(root: string): InstallResult {
  const path = join(root, ".claude", "settings.json");
  if (!existsSync(path)) return { path, changed: false, files: [] };
  const existing = readJsonObject(path);
  const settings: Record<string, unknown> = { ...existing };
  const hooks = isRecord(settings.hooks) ? { ...settings.hooks } : {};
  const stop = Array.isArray(hooks.Stop) ? hooks.Stop : [];
  const filtered = stop.filter((entry) => !isShrikeHookEntry(entry));
  if (filtered.length === 0) delete hooks.Stop;
  else hooks.Stop = filtered;
  if (Object.keys(hooks).length === 0) delete settings.hooks;
  else settings.hooks = hooks;
  const changed = writeJsonIfChanged(path, settings, existing);
  return { path, changed, files: [path] };
}
