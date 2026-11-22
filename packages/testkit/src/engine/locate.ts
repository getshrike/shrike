import { accessSync, constants } from "node:fs";
import { delimiter, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { EngineNotFoundError } from "./types.ts";

function isExecutable(path: string): boolean {
  try {
    accessSync(path, constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

function walkUpForEngine(start: string): string | null {
  let current = resolve(start);
  for (;;) {
    for (const profile of ["release", "debug"]) {
      const candidate = join(current, "target", profile, executableName("shrike-engine"));
      if (isExecutable(candidate)) return candidate;
    }
    const parent = dirname(current);
    if (parent === current) return null;
    current = parent;
  }
}

function executableName(base: string): string {
  return process.platform === "win32" ? `${base}.exe` : base;
}

function findOnPath(name: string): string | null {
  const pathValue = process.env.PATH ?? "";
  for (const directory of pathValue.split(delimiter)) {
    if (!directory) continue;
    const candidate = join(directory, executableName(name));
    if (isExecutable(candidate)) return candidate;
  }
  return null;
}

export interface LocateEngineOptions {
  cwd?: string;
  /** Override the package/repo root used to find target/. */
  repoRoot?: string;
}

/**
 * Locate `shrike-engine`: env SHRIKE_ENGINE, target/release, target/debug, PATH.
 * A set but invalid SHRIKE_ENGINE is an error rather than a silent fallback.
 */
export function locateEngine(options: LocateEngineOptions = {}): string {
  const override = process.env.SHRIKE_ENGINE;
  if (override && override.length > 0) {
    if (!isExecutable(override)) {
      throw new EngineNotFoundError(
        `SHRIKE_ENGINE points to a missing or non-executable file: ${override}`,
      );
    }
    return resolve(override);
  }

  const moduleRepoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
  const candidates = [
    options.repoRoot,
    walkUpForEngine(options.cwd ?? process.cwd()),
    walkUpForEngine(moduleRepoRoot),
  ].filter((value): value is string => Boolean(value));

  for (const root of candidates) {
    for (const profile of ["release", "debug"]) {
      const candidate = join(root, "target", profile, executableName("shrike-engine"));
      if (isExecutable(candidate)) return candidate;
    }
  }

  const onPath = findOnPath("shrike-engine");
  if (onPath) return onPath;

  throw new EngineNotFoundError(
    "could not find shrike-engine (set SHRIKE_ENGINE, build target/release/shrike-engine, or add it to PATH)",
  );
}
