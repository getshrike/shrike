import { HOOK_COMMAND, type HookTarget, type InstallResult } from "./common.ts";
import { installClaudeHook, uninstallClaudeHook } from "./claude.ts";
import { installCursorHook, uninstallCursorHook } from "./cursor.ts";
import { installOpenCodeHook, uninstallOpenCodeHook } from "./opencode.ts";

/** Remove one supported hook target. */
export function uninstallHook(root: string, target: HookTarget): InstallResult {
  switch (target) {
    case "claude":
      return uninstallClaudeHook(root);
    case "cursor":
      return uninstallCursorHook(root);
    case "opencode":
      return uninstallOpenCodeHook(root);
  }
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
