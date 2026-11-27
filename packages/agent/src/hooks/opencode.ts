import { existsSync, readFileSync, rmSync } from "node:fs";
import { join } from "node:path";

import {
  HookConfigError, OPENCODE_PLUGIN_MARKER, type InstallResult, isRecord, readJsonObject,
  writeFileAtomic, writeJsonIfChanged,
} from "./common.ts";

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
          await $\`npx getshrike hook run --quiet\`.quiet().nothrow()
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

/** Remove the OpenCode plugin and its registration. */
export function uninstallOpenCodeHook(root: string): InstallResult {
  const pluginPath = join(root, ".opencode", "plugins", "shrike-hook.js");
  const configPath = join(root, "opencode.json");
  let pluginChanged = false;
  if (existsSync(pluginPath)) {
    const current = readFileSync(pluginPath, "utf8");
    if (current.includes(OPENCODE_PLUGIN_MARKER)) {
      rmSync(pluginPath, { force: true });
      pluginChanged = true;
    }
  }
  let configChanged = false;
  if (existsSync(configPath)) {
    const existing = readJsonObject(configPath);
    const config: Record<string, unknown> = { ...existing };
    if (Array.isArray(config.plugin)) {
      const filtered = config.plugin.filter(
        (item) => item !== "./.opencode/plugins/shrike-hook.js",
      );
      if (filtered.length === 0) delete config.plugin;
      else config.plugin = filtered;
      configChanged = writeJsonIfChanged(configPath, config, existing);
    }
  }
  return {
    path: configPath,
    changed: pluginChanged || configChanged,
    files: [configPath, pluginPath],
  };
}
