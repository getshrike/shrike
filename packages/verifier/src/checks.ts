import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import type { CheckSpec, CheckTier } from "./types.ts";

/** Package managers the verifier can invoke. */
export type PackageManager = "pnpm" | "npm" | "yarn" | "bun";

export interface DetectChecksOptions {
  manager?: PackageManager;
  tier?: CheckTier;
  /** Base timeout for every check. Individual scripts get tier-aware defaults. */
  timeoutMs?: number;
  /** Override reruns for test-like checks. */
  testReruns?: number;
}

interface ScriptDefinition {
  name: string;
  /** Accepted package.json script names, in preference order. */
  scripts: string[];
  timeoutMs: number;
  reruns: number;
  tiers: CheckTier[];
}

const SCRIPT_DEFINITIONS: ScriptDefinition[] = [
  {
    name: "typecheck",
    scripts: ["typecheck", "type-check", "tsc"],
    timeoutMs: 180_000,
    reruns: 0,
    tiers: ["fast", "standard", "full"],
  },
  {
    name: "lint",
    scripts: ["lint"],
    timeoutMs: 180_000,
    reruns: 0,
    tiers: ["fast", "standard", "full"],
  },
  {
    name: "test",
    scripts: ["test"],
    timeoutMs: 600_000,
    reruns: 1,
    tiers: ["standard", "full"],
  },
  {
    name: "build",
    scripts: ["build"],
    timeoutMs: 900_000,
    reruns: 0,
    tiers: ["full"],
  },
];

/**
 * A watch/dev script never terminates on its own, so it cannot be a verifier
 * check. Detect the common forms and skip them instead of hanging a run.
 */
function isInteractiveScript(script: string): boolean {
  return /(^|[\s;&|])(--watch|-w|--watchAll|watch)([\s;&|]|$)/i.test(script) ||
    /\b(vite|webpack|next|nodemon)\s+(dev|serve)\b/i.test(script);
}

const TIER_RANK: Record<Exclude<CheckTier, "none">, number> = {
  fast: 0,
  standard: 1,
  full: 2,
};

/** Read the root package.json scripts. Returns an empty map when absent. */
export function readPackageScripts(root: string): Record<string, string> {
  const path = join(root, "package.json");
  if (!existsSync(path)) return {};
  try {
    const parsed: unknown = JSON.parse(readFileSync(path, "utf8"));
    if (
      typeof parsed === "object" &&
      parsed !== null &&
      "scripts" in parsed &&
      typeof parsed.scripts === "object" &&
      parsed.scripts !== null
    ) {
      const scripts: Record<string, string> = {};
      for (const [key, value] of Object.entries(parsed.scripts)) {
        if (typeof value === "string") scripts[key] = value;
      }
      return scripts;
    }
  } catch {
    return {};
  }
  return {};
}

/** Detect the package manager from `packageManager` or lockfiles. */
export function detectPackageManager(root: string): PackageManager {
  const packageJsonPath = join(root, "package.json");
  if (existsSync(packageJsonPath)) {
    try {
      const parsed: unknown = JSON.parse(readFileSync(packageJsonPath, "utf8"));
      if (
        typeof parsed === "object" &&
        parsed !== null &&
        "packageManager" in parsed &&
        typeof parsed.packageManager === "string"
      ) {
        const name = parsed.packageManager.split("@")[0];
        if (name === "pnpm" || name === "npm" || name === "yarn" || name === "bun") {
          return name;
        }
      }
    } catch {
      // Fall through to lockfile detection.
    }
  }

  if (existsSync(join(root, "pnpm-lock.yaml"))) return "pnpm";
  if (existsSync(join(root, "package-lock.json"))) return "npm";
  if (existsSync(join(root, "yarn.lock"))) return "yarn";
  if (existsSync(join(root, "bun.lockb")) || existsSync(join(root, "bun.lock"))) {
    return "bun";
  }
  return "pnpm";
}

function managerCommand(manager: PackageManager, script: string): string {
  if (manager === "npm") return `npm run ${script}`;
  return `${manager} run ${script}`;
}

/**
 * Detect checks from package.json scripts.
 *
 * Defaults match the product contract: typecheck and lint for the fast tier;
 * test is added for standard; build for full. Missing scripts are skipped.
 */
export function detectChecks(
  scripts: Record<string, string>,
  options: DetectChecksOptions = {},
): CheckSpec[] {
  const tier = options.tier ?? "fast";
  if (tier === "none") return [];
  const manager = options.manager ?? "pnpm";
  const rank = TIER_RANK[tier];
  const specs: CheckSpec[] = [];

  for (const definition of SCRIPT_DEFINITIONS) {
    const script = definition.scripts.find((name) => name in scripts);
    if (script === undefined) continue;
    const command = scripts[script] ?? "";
    if (isInteractiveScript(command)) continue;
    if (TIER_RANK[definition.tiers[0] as Exclude<CheckTier, "none">] > rank) continue;
    specs.push({
      name: definition.name,
      command: managerCommand(manager, script),
      timeoutMs: options.timeoutMs ?? definition.timeoutMs,
      reruns:
        definition.name === "test"
          ? options.testReruns ?? definition.reruns
          : definition.reruns,
      required: true,
    });
  }

  return specs;
}

/** Detect checks for a repository root. */
export function detectRepositoryChecks(
  root: string,
  options: DetectChecksOptions = {},
): CheckSpec[] {
  const scripts = readPackageScripts(root);
  return detectChecks(scripts, {
    ...options,
    manager: options.manager ?? detectPackageManager(root),
  });
}
