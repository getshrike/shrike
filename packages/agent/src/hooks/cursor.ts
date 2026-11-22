import { existsSync } from "node:fs";
import { join } from "node:path";

import {
  HOOK_COMMAND, type InstallResult, isRecord, isShrikeHookEntry, readJsonObject,
  writeJsonIfChanged,
} from "./common.ts";

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

/** Remove only Shrike-managed entries from a Cursor hooks file. */
export function uninstallCursorHook(root: string): InstallResult {
  const path = join(root, ".cursor", "hooks.json");
  if (!existsSync(path)) return { path, changed: false, files: [] };
  const existing = readJsonObject(path);
  const config: Record<string, unknown> = { ...existing };
  const hooks = isRecord(config.hooks) ? { ...config.hooks } : {};
  for (const event of ["afterFileEdit", "stop"] as const) {
    if (!Array.isArray(hooks[event])) continue;
    const filtered = (hooks[event] as unknown[]).filter((entry) => !isShrikeHookEntry(entry));
    if (filtered.length === 0) delete hooks[event];
    else hooks[event] = filtered;
  }
  if (Object.keys(hooks).length === 0) delete config.hooks;
  else config.hooks = hooks;
  const changed = writeJsonIfChanged(path, config, existing);
  return { path, changed, files: [path] };
}
