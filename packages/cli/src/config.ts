import { spawnSync } from "node:child_process";
import { randomUUID } from "node:crypto";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, extname, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { ConfigError } from "./errors.ts";

export interface CheckConfig {
  name: string;
  command: string;
  timeoutMs?: number;
  reruns?: number;
  required?: boolean;
  tier?: VerificationTier;
  cwd?: string;
}

export type VerificationTier = "fast" | "standard" | "full";
export type VerificationMode = "worktree" | "in-place";
export type ClientBoundaryPolicy = "graph" | "protect";
export type ServerActionPolicy = "graph" | "protect";

export interface VerificationConfig {
  mode: VerificationMode;
  allowDirty: boolean;
  tier: VerificationTier;
  baselinePath: string;
}

export interface BuildConfig {
  evidence: boolean;
  snapshot: string;
  requireEvidence: boolean;
}

export interface PrConfig {
  draft: boolean;
  labels: string[];
  branchPrefix: string;
  commentReceipt: boolean;
  base: string | null;
  title: string;
}

export interface ShrikeConfig {
  schemaVersion: "1.0";
  include: string[];
  ignore: string[];
  tsconfig: string | null;
  cacheDir: string;
  entryPolicy: "local" | "production";
  testEntries: boolean;
  includeGenerated: boolean;
  /**
   * `graph` (default) treats a `"use client"` directive as a boundary marker
   * and lets normal reachability decide liveness. `protect` also seeds every
   * export in client-boundary files as an entry (the pre-1.1 fail-closed
   * behavior).
   */
  clientBoundaryPolicy: ClientBoundaryPolicy;
  /**
   * `graph` (default) treats a `"use server"` export as an ordinary export and
   * reports actions with no project reference. `protect` seeds every export in
   * a server-action module as an entry, preserving the pre-1.2 fail-closed
   * behavior for actions that remain POST-able if their id leaks.
   */
  serverActionPolicy: ServerActionPolicy;
  /** Dependency names that must never be reported as unused. */
  ignoreDependencies: string[];
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
  pr: PrConfig;
  verification: VerificationConfig;
  build: BuildConfig;
}

export const DEFAULT_CONFIG: ShrikeConfig = {
  schemaVersion: "1.0",
  include: [
    "**/*.ts",
    "**/*.tsx",
    "**/*.js",
    "**/*.jsx",
    "**/*.mjs",
    "**/*.cjs",
    "**/*.mdx",
    "**/*.svelte",
    "**/*.vue",
    "**/*.astro",
    "**/index.html",
  ],
  ignore: [
    "node_modules/**", ".git/**", ".next/**", ".shrike/**", "dist/**", "build/**",
    "out/**", "coverage/**", ".turbo/**", ".vercel/**", "*.min.js", "*.generated.*",
  ],
  tsconfig: "tsconfig.json",
  cacheDir: ".shrike/cache",
  entryPolicy: "local",
  testEntries: true,
  includeGenerated: false,
  clientBoundaryPolicy: "graph",
  serverActionPolicy: "graph",
  ignoreDependencies: [],
  publicApi: [],
  generated: [],
  batchPolicy: { maxSymbols: 50, maxFiles: 10, maxLiveGraphPercent: 5, allowOversized: false },
  entryAdapters: ["next-app", "next-pages", "react", "packages", "tests", "configs"],
  checks: [],
  pr: {
    draft: true,
    labels: ["shrike", "cleanup"],
    branchPrefix: "shrike/cleanup",
    commentReceipt: true,
    base: null,
    title: "Shrike: verified cleanup",
  },
  verification: {
    mode: "worktree",
    allowDirty: false,
    tier: "standard",
    baselinePath: ".shrike/baseline.json",
  },
  build: {
    evidence: false,
    snapshot: ".next/analyze.jsonl",
    requireEvidence: false,
  },
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
    schemaVersion: "1.0",
    batchPolicy: { ...DEFAULT_CONFIG.batchPolicy, ...(input.batchPolicy ?? {}) },
    pr: { ...DEFAULT_CONFIG.pr, ...(input.pr ?? {}) },
    verification: { ...DEFAULT_CONFIG.verification, ...(input.verification ?? {}) },
    build: { ...DEFAULT_CONFIG.build, ...(input.build ?? {}) },
    include: input.include ?? DEFAULT_CONFIG.include,
    ignore: input.ignore ?? DEFAULT_CONFIG.ignore,
    checks: input.checks ?? DEFAULT_CONFIG.checks,
  };
}

/**
 * Evaluate an executable config in a child process.
 *
 * `shrike.config.ts` is project code. Running it in-process would let it block
 * the CLI, crash the host, or read process state. The child boundary enforces a
 * hard timeout and keeps a malformed config from taking the verifier down.
 */
interface ConfigCandidate {
  url: string;
  cleanup: () => void;
}

/**
 * Node chooses a `.ts` file's module format from the nearest `package.json`.
 * A CommonJS project therefore treats an ESM `shrike.config.ts` as CommonJS
 * and fails on `export default`. Copying the config to a temporary `.mts`
 * (always ESM) in the same directory keeps relative imports working while
 * removing that package-level ambiguity. A `.cts` fallback supports a config
 * that uses CommonJS syntax.
 */
function executableConfigCandidates(path: string): ConfigCandidate[] {
  const extension = extname(path).toLowerCase();
  const candidates: ConfigCandidate[] = [];
  const copy = (suffix: string): void => {
    const temporary = join(dirname(path), `.shrike-config-${randomUUID()}${suffix}`);
    try {
      writeFileSync(temporary, readFileSync(path, "utf8"));
      candidates.push({
        url: pathToFileURL(temporary).href,
        cleanup: () => rmSync(temporary, { force: true }),
      });
    } catch {
      // A read-only directory cannot host a temporary copy; try the original.
    }
  };
  if (extension === ".ts" || extension === ".mts" || extension === ".cts") {
    copy(".mts");
    copy(".cts");
  } else {
    candidates.push({ url: pathToFileURL(path).href, cleanup: () => {} });
    if (extension === ".js" || extension === ".jsx") copy(".mjs");
  }
  if (candidates.length === 0) {
    candidates.push({ url: pathToFileURL(path).href, cleanup: () => {} });
  }
  return candidates;
}

async function loadExecutableConfig(path: string, root: string): Promise<Partial<ShrikeConfig>> {
  const { baseEnvironment } = await import("@shrike/verifier");
  const directory = mkdtempSync(join(tmpdir(), "shrike-config-"));
  const output = join(directory, "config.json");
  const timeout = Number(process.env.SHRIKE_CONFIG_TIMEOUT_MS ?? 5_000);
  const script = `
    import { writeFileSync } from "node:fs";
    const loaded = await import(process.env.SHRIKE_CONFIG_URL);
    const value = loaded.default ?? loaded.config ?? {};
    writeFileSync(process.env.SHRIKE_CONFIG_OUTPUT, JSON.stringify(value));
  `;
  const candidates = executableConfigCandidates(path);
  let lastError: string | undefined;
  try {
    for (const candidate of candidates) {
      try {
        const result = spawnSync(process.execPath, ["--input-type=module", "-e", script], {
          cwd: root,
          encoding: "utf8",
          timeout: Number.isFinite(timeout) && timeout > 0 ? timeout : 5_000,
          env: {
            ...baseEnvironment(),
            SHRIKE_CONFIG_URL: candidate.url,
            SHRIKE_CONFIG_OUTPUT: output,
          },
        });
        if (result.error) {
          const code = (result.error as NodeJS.ErrnoException).code;
          if (code === "ETIMEDOUT") {
            throw new ConfigError(`config evaluation timed out after ${timeout}ms: ${path}`);
          }
          lastError = result.error.message;
          continue;
        }
        if (result.signal) {
          throw new ConfigError(`config evaluation was terminated (${result.signal}): ${path}`);
        }
        if (result.status !== 0) {
          lastError = (result.stderr ?? "").trim().split("\n").slice(-5).join("\n");
          continue;
        }
        if (!existsSync(output)) {
          lastError = "config did not produce a value";
          continue;
        }
        const parsed: unknown = JSON.parse(readFileSync(output, "utf8"));
        if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
          throw new ConfigError(`config must export an object: ${path}`);
        }
        return parsed as Partial<ShrikeConfig>;
      } finally {
        rmSync(output, { force: true });
      }
    }
    throw new ConfigError(
      `config evaluation failed: ${path}`,
      lastError || "no executable config form succeeded",
    );
  } catch (error) {
    if (error instanceof ConfigError) throw error;
    throw new ConfigError(
      `could not load ${path}: ${error instanceof Error ? error.message : String(error)}`,
    );
  } finally {
    for (const candidate of candidates) candidate.cleanup();
    rmSync(directory, { recursive: true, force: true });
  }
}

export async function loadConfig(root: string): Promise<ShrikeConfig> {
  const path = findConfigPath(root);
  if (!path) return mergeConfig();
  if (path.endsWith(".json")) {
    try {
      const parsed: unknown = JSON.parse(readFileSync(path, "utf8"));
      if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
        throw new ConfigError(`config must be an object: ${path}`);
      }
      return mergeConfig(parsed as Partial<ShrikeConfig>);
    } catch (error) {
      if (error instanceof ConfigError) throw error;
      throw new ConfigError(
        `could not load ${path}: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
  }
  return mergeConfig(await loadExecutableConfig(path, root));
}

/** The payload shape accepted by the Rust engine's analyze/scan commands. */
export function engineConfig(config: ShrikeConfig): Record<string, unknown> {
  const {
    checks: _checks,
    pr: _pr,
    verification: _verification,
    build: _build,
    cacheDir: _cacheDir,
    schemaVersion: _schemaVersion,
    ignoreDependencies: _ignoreDependencies,
    ...rest
  } = config;
  return rest;
}

/** Resolve the configured cache directory to an absolute host path. */
export function cacheDirectory(root: string, config: ShrikeConfig): string {
  return config.cacheDir.startsWith("/") ? config.cacheDir : join(root, config.cacheDir);
}

export function repoRoot(cwd = process.cwd()): string {
  return resolve(cwd);
}
