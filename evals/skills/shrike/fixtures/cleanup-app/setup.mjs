#!/usr/bin/env node
/**
 * Build the cleanup eval fixture as a deterministic git repository.
 *
 * Usage: node setup.mjs <destination>
 *
 * The script copies `base/`, creates the base commit, checks out
 * `feature/remove-invoice`, applies the `head/` overlay, and creates the
 * feature commit. Commit dates, authors, messages, and content are fixed so
 * two runs produce the same history hashes and the same analysis input.
 */
import { spawnSync } from "node:child_process";
import { cpSync, mkdirSync, rmSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const destination = process.argv[2];
if (!destination) {
  console.error("usage: node setup.mjs <destination>");
  process.exit(2);
}

const target = resolve(destination);
rmSync(target, { recursive: true, force: true });
mkdirSync(target, { recursive: true });
cpSync(join(here, "base"), target, { recursive: true });

const GIT_ENV = {
  ...process.env,
  GIT_AUTHOR_NAME: "Shrike Evals",
  GIT_AUTHOR_EMAIL: "evals@shrike.invalid",
  GIT_COMMITTER_NAME: "Shrike Evals",
  GIT_COMMITTER_EMAIL: "evals@shrike.invalid",
  GIT_AUTHOR_DATE: "2026-01-01T00:00:00Z",
  GIT_COMMITTER_DATE: "2026-01-01T00:00:00Z",
  GIT_CONFIG_NOSYSTEM: "1",
};

function git(args) {
  const result = spawnSync("git", args, {
    cwd: target,
    encoding: "utf8",
    env: GIT_ENV,
  });
  if (result.status !== 0) {
    console.error(`git ${args.join(" ")} failed:\n${result.stderr.trim()}`);
    process.exit(1);
  }
  return result.stdout.trim();
}

git(["init", "--quiet", "--initial-branch=main"]);
git(["config", "user.name", "Shrike Evals"]);
git(["config", "user.email", "evals@shrike.invalid"]);
git(["add", "--all"]);
git(["commit", "--quiet", "-m", "fixture: base app with existing dead code"]);
git(["checkout", "--quiet", "-b", "feature/remove-invoice"]);
cpSync(join(here, "head"), target, { recursive: true });
git(["add", "--all"]);
git(["commit", "--quiet", "-m", "fixture: stop rendering the invoice"]);

console.log(`OK: cleanup eval fixture at ${target}`);
console.log(`  base:    ${git(["rev-parse", "main"])}`);
console.log(`  branch:  ${git(["rev-parse", "--abbrev-ref", "HEAD"])}`);
console.log(`  head:    ${git(["rev-parse", "HEAD"])}`);
