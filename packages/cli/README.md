# shrike

The cleanup layer for AI-built TypeScript apps: dead-weight report, verified
deletion batches, and portable receipts.

```bash
npm install -D getshrike
npx getshrike init
npx getshrike report
```

`report` is the read-only entry point: it measures dead weight in the current
tree, or run `npx getshrike report --base origin/main` to see what a branch
orphaned. Run `npx getshrike` with no arguments on an interactive terminal to
open the read-only view; in CI, pipes, and agent sessions the output is the
deterministic plain report.

Read-only evidence and planning:

```bash
shrike scan
shrike delta --base origin/main
shrike plan --base origin/main
shrike why src/lib/format.ts#formatInvoice
```

`shrike clean --base origin/main` (or its alias `prune`) verifies each batch in
a worktree, reverts failed edits, and writes a receipt; add `--pr` to open a
draft PR when `gh` is available. `shrike clean --all` runs the same verified
pipeline for the current tree. Neither commits to the base branch. `shrike
install` writes the portable skill directory (a short `SKILL.md` plus four
one-level-deep references) and `shrike ci install` writes the advisory GitHub
Actions workflow. Reinstalling identical files reports `unchanged`; local edits
are skipped until `--force`, and one conflict blocks every skill file so the
installed directory never mixes versions.

The native engine is a Rust binary. The launcher resolves it from
`SHRIKE_ENGINE`, a repository `target/{release,debug}` build, the installed
platform package, then `PATH`. A missing engine fails closed with exit code 4.

See the [repository README](https://github.com/getshrike/shrike#readme),
[architecture](https://github.com/getshrike/shrike/blob/main/docs/architecture.md),
and [consumer docs](https://github.com/getshrike/shrike/tree/main/apps/docs) for
hooks, the Action, MCP, and the trust boundary.
