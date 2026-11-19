import { existsSync, readFileSync, readdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/** Repository root, derived from this package's location. */
export function repositoryRoot(): string {
  return resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
}

export function fixturesDir(...segments: string[]): string {
  return join(repositoryRoot(), "fixtures", ...segments);
}

/** True when a fixture exists. Tests use this to tolerate parallel creation. */
export function fixtureExists(...segments: string[]): boolean {
  return existsSync(fixturesDir(...segments));
}

/** Read a JSON fixture. Throws with a clear path when absent. */
export function loadFixtureJson<T = unknown>(...segments: string[]): T {
  const path = fixturesDir(...segments);
  const parsed: unknown = JSON.parse(readFileSync(path, "utf8"));
  return parsed as T;
}

/** Read a JSON fixture if present; null when another agent has not created it. */
export function tryLoadFixtureJson<T = unknown>(...segments: string[]): T | null {
  const path = fixturesDir(...segments);
  if (!existsSync(path)) return null;
  const parsed: unknown = JSON.parse(readFileSync(path, "utf8"));
  return parsed as T;
}

/** List files under a fixture directory, sorted for determinism. */
export function listFixtureFiles(...segments: string[]): string[] {
  const directory = fixturesDir(...segments);
  if (!existsSync(directory)) return [];
  const files: string[] = [];
  const walk = (current: string): void => {
    for (const entry of readdirSync(current, { withFileTypes: true })) {
      const path = join(current, entry.name);
      if (entry.isDirectory()) walk(path);
      else files.push(path);
    }
  };
  walk(directory);
  return files.sort();
}

/** Parse JSON Lines, ignoring blank lines. */
export function parseJsonLines(input: string): unknown[] {
  return input
    .split(/\r?\n/)
    .filter((line) => line.trim().length > 0)
    .map((line) => JSON.parse(line) as unknown);
}
