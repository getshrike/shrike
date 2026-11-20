import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { isAbsolute, join, resolve } from "node:path";

import { resolveInside, safeRelativePath } from "../recovery.ts";
import { normalizeBuildEvidence, streamBuildEvidence } from "./normalize.ts";
import { sha256 } from "./paths.ts";
import { discoverBuildEvidence, packageNextVersion } from "./records.ts";
import {
  BuildEvidenceError, EMPTY_BUILD_HASH,
  type BuildEvidence, type BuildEvidenceOptions, type LoadedBuildEvidence,
} from "./types.ts";

function emptyEvidence(root: string): BuildEvidence {
  return {
    source: "static",
    version: packageNextVersion(root) || "unknown",
    files: [],
    symbols: [],
    routes: [],
    sizes: {},
    snapshotHash: EMPTY_BUILD_HASH,
    layers: {},
    moduleIds: {},
    clientReferences: [],
    fileHashes: {},
    unknownRecords: 0,
  };
}

function applyStaleness(root: string, evidence: BuildEvidence): void {
  const entries = Object.entries(evidence.fileHashes);
  if (entries.length === 0) return;
  for (const [file, expected] of entries) {
    const full = join(root, file);
    if (!existsSync(full)) {
      evidence.status = "stale";
      return;
    }
    const actual = sha256(readFileSync(full));
    const normalized = expected.startsWith("sha256:") ? expected : `sha256:${expected}`;
    if (actual !== normalized) {
      evidence.status = "stale";
      return;
    }
  }
}

function runBuild(root: string, options: BuildEvidenceOptions): string | null {
  const command = options.buildCommand ?? "npm run build";
  const result = spawnSync(command, {
    cwd: root,
    shell: true,
    encoding: "utf8",
    timeout: options.timeoutMs ?? 600_000,
    env: process.env,
  });
  if (result.error) return result.error.message;
  if (result.status !== 0) {
    const detail = (result.stderr || result.stdout || `exit ${result.status}`).trim();
    return detail.slice(0, 2_000);
  }
  return null;
}

/** Load explicit evidence or discover a default snapshot. */
export function loadBuildEvidence(
  root: string,
  explicitPath?: string | null,
  options: BuildEvidenceOptions = {},
): LoadedBuildEvidence {
  if (options.runBuild || options.buildCommand) {
    const failure = runBuild(root, options);
    if (failure) {
      return {
        evidence: { ...emptyEvidence(root), status: "build-failed" },
        path: null,
        error: null,
        failure,
      };
    }
  }

  let path: string | null;
  try {
    path = explicitPath ? resolveEvidencePath(root, explicitPath) : discoverBuildEvidence(root);
  } catch (error) {
    if (error instanceof BuildEvidenceError) {
      return { evidence: emptyEvidence(root), path: null, error: error.message, failure: null };
    }
    throw error;
  }
  if (!path) {
    return { evidence: emptyEvidence(root), path: null, error: null, failure: null };
  }
  try {
    const evidence = path.endsWith(".jsonl")
      ? streamBuildEvidence(root, path)
      : normalizeBuildEvidence(root, readFileSync(path, "utf8"), path);
    applyStaleness(root, evidence);
    return { evidence, path, error: null, failure: null };
  } catch (error) {
    if (error instanceof BuildEvidenceError) {
      return { evidence: emptyEvidence(root), path, error: error.message, failure: null };
    }
    if ((error as NodeJS.ErrnoException).code === "ENOENT") {
      return {
        evidence: emptyEvidence(root),
        path,
        error: `build evidence does not exist: ${path}`,
        failure: null,
      };
    }
    throw error;
  }
}

function resolveEvidencePath(root: string, input: string): string {
  if (isAbsolute(input)) {
    const normalized = resolve(input);
    const base = resolve(root);
    if (normalized !== base && !normalized.startsWith(`${base}/`)) {
      throw new BuildEvidenceError("malformed", `build evidence is outside the repository: ${input}`);
    }
    return normalized;
  }
  try {
    return resolveInside(root, safeRelativePath(input));
  } catch (error) {
    throw new BuildEvidenceError(
      "malformed",
      error instanceof Error ? error.message : String(error),
    );
  }
}
