import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";

export function git(root: string, args: string[], allowFailure = false): string {
  try {
    return execFileSync("git", args, { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
  } catch (error) {
    if (allowFailure) return "";
    const message = error instanceof Error && "stderr" in error ? String((error as { stderr?: unknown }).stderr) : String(error);
    throw new Error(`git ${args.join(" ")} failed: ${message.trim()}`);
  }
}

export function isDirty(root: string): boolean {
  // Shrike's own artifacts are never part of the project's review surface.
  return git(root, ["status", "--porcelain"], true)
    .split("\n")
    .filter((line) => line.trim().length > 0)
    .some((line) => !line.slice(3).startsWith(".shrike/"));
}

export function headCommit(root: string): string {
  return git(root, ["rev-parse", "HEAD"]);
}

export function mergeBase(root: string, base: string): string {
  const merge = git(root, ["merge-base", base, "HEAD"], true);
  if (merge) return merge;
  const fallback = git(root, ["rev-parse", base], true);
  if (fallback) return fallback;
  throw new Error(`could not resolve base ref ${base}`);
}

export function listRemotes(root: string): string[] {
  return git(root, ["remote"], true)
    .split("\n")
    .map((name) => name.trim())
    .filter((name) => name.length > 0);
}

/**
 * `gh pr create --base` expects a branch name, but a plan records the ref it
 * analyzed, which is usually the remote-tracking `origin/main`. Strip a
 * leading remote name only when it is a configured remote, so a branch such
 * as `release/1.0` is left alone.
 */
export function pullRequestBase(baseRef: string, remotes: string[]): string {
  const slash = baseRef.indexOf("/");
  if (slash <= 0) return baseRef;
  const remote = baseRef.slice(0, slash);
  return remotes.includes(remote) ? baseRef.slice(slash + 1) : baseRef;
}

export function resolveDefaultBase(root: string, explicit?: string): string {
  if (explicit) return explicit;
  for (const candidate of ["origin/main", "main", "origin/master", "master"]) {
    if (git(root, ["rev-parse", "--verify", candidate], true)) return candidate;
  }
  throw new Error("could not find a base branch; pass --base <ref>");
}

export interface Worktree {
  directory: string;
  cleanup: () => void;
}

function worktreeDirectory(root: string, label: string): string {
  const directory = join(root, ".shrike", "worktrees");
  mkdirSync(directory, { recursive: true });
  // Stale registrations are metadata-only; prune before adding a new one.
  git(root, ["worktree", "prune"], true);
  return mkdtempSync(join(directory, `${label}-`));
}

/**
 * Cleanup worktrees are named after the content-addressed plan, so their path
 * is deterministic and safe to print in JSON. A leftover directory is a real
 * local change; refuse to delete it silently.
 */
function branchWorktreeDirectory(root: string, label: string): string {
  const directory = join(root, ".shrike", "worktrees");
  mkdirSync(directory, { recursive: true });
  git(root, ["worktree", "prune"], true);
  const target = join(directory, label);
  if (existsSync(target)) {
    throw new Error(`worktree directory already exists: ${target}; remove it and retry`);
  }
  return target;
}

/** Create a detached worktree for a commit. */
export function createWorktree(root: string, commit: string, label: string): Worktree {
  const directory = worktreeDirectory(root, label);
  try {
    git(root, ["worktree", "add", "--detach", directory, commit]);
  } catch (error) {
    rmSync(directory, { recursive: true, force: true });
    throw error;
  }
  return {
    directory,
    cleanup: () => {
      try {
        git(root, ["worktree", "remove", "--force", directory]);
      } catch {
        // best effort
      }
      try { rmSync(directory, { recursive: true, force: true }); } catch { /* best effort */ }
    },
  };
}

/** Create a new branch in a temporary worktree (default prune mode). */
export function createBranchWorktree(root: string, branch: string, commit: string): Worktree {
  const directory = branchWorktreeDirectory(root, branch.replace(/[^a-zA-Z0-9]+/g, "-"));
  const exists = git(root, ["rev-parse", "--verify", branch], true);
  try {
    if (exists) git(root, ["worktree", "add", directory, branch]);
    else git(root, ["worktree", "add", "-b", branch, directory, commit]);
  } catch (error) {
    rmSync(directory, { recursive: true, force: true });
    throw error;
  }
  return {
    directory,
    cleanup: () => {
      try { git(root, ["worktree", "remove", "--force", directory]); } catch { /* best effort */ }
      try { rmSync(directory, { recursive: true, force: true }); } catch { /* best effort */ }
    },
  };
}

export function currentBranch(root: string): string {
  return git(root, ["rev-parse", "--abbrev-ref", "HEAD"], true);
}

export function ensureBranch(root: string, branch: string): void {
  if (currentBranch(root) === branch) return;
  const existing = git(root, ["rev-parse", "--verify", branch], true);
  if (existing) git(root, ["checkout", branch]);
  else git(root, ["checkout", "-b", branch]);
}

export function commitAll(root: string, message: string): void {
  git(root, ["add", "-A"]);
  git(root, ["commit", "-m", message]);
}

export function restoreAll(root: string): void {
  git(root, ["checkout", "--", "."]);
  git(root, ["clean", "-fd", "--", "."]);
}
