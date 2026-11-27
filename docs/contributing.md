# Contributing: commits, PRs, and review

## Branch and commit rules

- Do not commit to `main`. Branch from it, keep each change to one concern, and
  open a PR.
- Titles follow Conventional Commits: `feat:`, `fix:`, `docs:`, `test:`,
  `refactor:`, `perf:`, `build:`, `ci:`, `chore:` (optional scope), then an
  imperative summary. Example: `fix(graph): protect custom route entries`.
- Explain *why* in the PR description or commit body; the diff shows *what*.
- No secrets, credentials, or generated build output in commits.

## Before opening a PR

```bash
pnpm -r typecheck && pnpm run lint && pnpm test
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
node scripts/check.mjs
node scripts/check-docs.mjs && node scripts/check-links.mjs
```

Use the on-demand `create-pr` skill for the full workflow. Fill in
[`.github/pull_request_template.md`](../.github/pull_request_template.md):
summary, changes, commands run with their results, risk, and the checklist.
Verification evidence lives in the PR and in CI — there is no status tracker to
update.

Checklist intent:

- The change is scoped to one concern and has no unrelated reformatting.
- Behavior changes have a test or fixture ([testing.md](testing.md)) and the
  relevant docs ([README.md](../README.md), [architecture.md](architecture.md))
  are updated in the same PR.
- Schema, protocol, or `ANALYSIS_VERSION` changes are applied consistently on
  both sides of the engine/host boundary.
- No generated files were hand-edited; the committed Action bundle is current.

## What reviewers look for

1. **Correctness and false positives** — liveness is conservative; a new
   protection is backed by a corpus case, and a new report has a witness.
2. **Determinism** — stable ordering, no host-specific data in JSON artifacts.
3. **Contract integrity** — `schemas/` and both runtimes agree; version bumps
   are included where required.
4. **Blast radius** — no new I/O in pure crates, no circular package imports,
   no weakening of the verification/receipt contract.
5. **Evidence** — tests added, gates run, output shown in the PR.

Reviewers should run the `code-review` skill; it encodes this checklist as a
repeatable procedure.

## Code quality rules

The always-on rules in [`.agent/rules/`](../.agent/rules/) apply to every change:

- [code-quality.md](../.agent/rules/code-quality.md) — formatting, types,
  error handling, generated files, determinism.
- [testing.md](../.agent/rules/testing.md) — regression coverage and gate runs.
- [git-and-pr.md](../.agent/rules/git-and-pr.md) — commits, scope, PR template.

They are advisory prose for humans and loaded instructions for agents via
`opencode.jsonc`; keep them short and in sync with this page.
