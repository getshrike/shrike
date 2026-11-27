import { existsSync } from "node:fs";
import { join } from "node:path";

export type InstallTarget =
  | "agents"
  | "claude"
  | "cursor"
  | "opencode"
  | "codex"
  | "gemini"
  | "copilot";

export const INSTALL_TARGETS: readonly InstallTarget[] = [
  "agents",
  "claude",
  "codex",
  "copilot",
  "cursor",
  "gemini",
  "opencode",
];

export function parseTarget(value: string): InstallTarget[] {
  return value
    .split(",")
    .map((entry) => entry.trim().toLowerCase())
    .filter((entry) => entry.length > 0)
    .map((entry) => {
      if (!(INSTALL_TARGETS as readonly string[]).includes(entry)) {
        throw new Error(`unsupported target: ${entry}`);
      }
      return entry as InstallTarget;
    });
}

/** Detect agent targets from repository markers only. Never inspects the network. */
export function detectTargets(root: string): InstallTarget[] {
  const has = (path: string): boolean => existsSync(join(root, path));
  const detected: InstallTarget[] = [];
  if (has(".claude") || has("CLAUDE.md")) detected.push("claude");
  if (has(".cursor") || has(".cursorrules")) detected.push("cursor");
  if (has("opencode.jsonc") || has("opencode.json") || has(".opencode")) detected.push("opencode");
  if (has(".codex")) detected.push("codex");
  if (has(".gemini") || has("GEMINI.md")) detected.push("gemini");
  if (has(".github/copilot-instructions.md")) detected.push("copilot");
  return detected;
}
