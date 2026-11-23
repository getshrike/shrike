---
name: create-pr
description: Prepare and open a scoped pull request with a Conventional Commit title, the repository PR template, and verified gate results. Use when asked to create, open, or submit a PR for this repository.
---

# Create a pull request

## 1. Establish the scope

- `git status --short`, `git diff main...HEAD --stat`, and `git log main..HEAD --oneline`.
- The branch must address one concern. If it mixes unrelated work, stop and
  propose a split before continuing.
- Never target a branch other than `main` unless the user says otherwise. Never
  commit to `main` directly.

## 2. Verify

- Run the gates that match the change, then the full local suite. See
  [gates](references/gates.md) for the command mapping.
- Capture the exact commands and their output for the PR body. Do not write
  "tests pass" without evidence.
- Check `git diff --exit-code packages/action/dist/main.cjs` after
  `pnpm run build:ts` if TypeScript output changed.
- If behavior changed, confirm the relevant `docs/` page was updated in this
  PR and that `node scripts/check-docs.mjs` and `node scripts/check-links.mjs`
  pass.

## 3. Check the diff for leakage

- No secrets, credentials, local absolute paths, or generated build output.
- No unrelated reformatting, no weakened tests, no disabled checks.
- No reintroduced `TRACKER.md`, plans, prompts, or status logs.

## 4. Write the PR

- Title: `<type>[scope]: <imperative summary>` using Conventional Commits.
- Body: follow [`.github/pull_request_template.md`](../../../.github/pull_request_template.md):
  Summary, Changes, Testing (commands + results), Risk, Checklist.
- State blockers plainly in the body; there is no status tracker to update.

## 5. Publish

- Commit with the same Conventional Commit title if uncommitted work remains.
- Push the branch, then open the PR with `gh pr create --fill` or an explicit
  `--title`/`--body`. Report the resulting URL.
