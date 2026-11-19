import { existsSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { ConfigError } from "./errors.ts";

export interface CheckConfig {
  name: string;
  command: string;
  timeoutMs?: number;
  reruns?: number;
  required?: boolean;
}

export interface ShrikeConfig {
  include: string[];
  ignore: string[];
  tsconfig: string | null;
  entryPolicy: "local" | "production";
  testEntries: boolean;
  includeGenerated: boolean;
  publicApi: string[];
  generated: string[];
  batchPolicy: {
    maxSymbols: number;
    maxFiles: number;
    maxLiveGraphPercent: number;
    allowOversized: boolean;
  };
  entryAdapters: string[];
  checks: CheckConfig[];
  pr: { draft: boolean; base: string | null };
}

export const DEFAULT_CONFIG: ShrikeConfig = {
  include: ["**/*.ts", "**/*.tsx", "**/*.js", "**/*.jsx", "**/*.mjs", "**/*.cjs"],
  ignore: [
    "node_modules/**", ".git/**", ".next/**", ".shrike/**", "dist/**", "build/**",
    "out/**", "coverage/**", ".turbo/**", ".vercel/**", "*.min.js", "*.generated.*",
  ],
  tsconfig: "tsconfig.json",
  entryPolicy: "local",
  testEntries: true,
  includeGenerated: false,
  publicApi: [],
  generated: [],
  batchPolicy: { maxSymbols: 50, maxFiles: 10, maxLiveGraphPercent: 5, allowOversized: false },
  entryAdapters: ["next-app", "next-pages", "react", "packages", "tests", "configs"],
  checks: [],
  pr: { draft: true, base: null },
};

const CONFIG_FILES = ["shrike.config.ts", "shrike.config.js", "shrike.config.mjs", "shrike.config.json"];

export function findConfigPath(root: string): string | null {
  for (const name of CONFIG_FILES) {
    const path = join(root, name);
    if (existsSync(path)) return path;
  }
  return null;
}

export function mergeConfig(input: Partial<ShrikeConfig> = {}): ShrikeConfig {
  return {
    ...DEFAULT_CONFIG,
    ...input,
    batchPolicy: { ...DEFAULT_CONFIG.batchPolicy, ...(input.batchPolicy ?? {}) },
    pr: { ...DEFAULT_CONFIG.pr, ...(input.pr ?? {}) },
    include: input.include ?? DEFAULT_CONFIG.include,
    ignore: input.ignore ?? DEFAULT_CONFIG.ignore,
    checks: input.checks ?? DEFAULT_CONFIG.checks,
  };
}

export async function loadConfig(root: string): Promise<ShrikeConfig> {
  const path = findConfigPath(root);
  if (!path) return mergeConfig();
  try {
    if (path.endsWith(".json")) {
      return mergeConfig(JSON.parse(readFileSync(path, "utf8")) as Partial<ShrikeConfig>);
    }
    const module = (await import(`${pathToFileURL(path).href}?t=${Date.now()}`)) as {
      default?: Partial<ShrikeConfig>;
      config?: Partial<ShrikeConfig>;
    };
    return mergeConfig(module.default ?? module.config ?? {});
  } catch (error) {
    throw new ConfigError(
      `could not load ${path}: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
}

/** The payload shape accepted by the Rust engine's analyze/scan commands. */
export function engineConfig(config: ShrikeConfig): Record<string, unknown> {
  const { checks: _checks, pr: _pr, ...rest } = config;
  return rest;
}

export function repoRoot(cwd = process.cwd()): string {
  return resolve(cwd);
}
