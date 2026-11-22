# Shrike design-partner trial

This kit is for a read-only first trial. It runs the whole-graph delta and the
deletion plan, writes reports under `.shrike/trial/`, and never edits, deletes,
commits, or opens a pull request.

## Before you start

- A Git repository with a `main` branch (or set `SHRIKE_BASE`).
- Node.js 22 or newer.
- A built Shrike engine. From the Shrike checkout:
  `cargo build --release -p shrike-engine`, then set
  `SHRIKE_ENGINE=/path/to/shrike/target/release/shrike-engine`.
- A `shrike.config.json` (or `shrike.config.ts`) with the checks your team
  trusts. The trial does not need checks; it only reads the graph. Checks are
  required before any `prune`.

## One command

From the repository you want to evaluate:

```bash
/path/to/shrike/design-partner/trial.sh
```

Or pass a path and base explicitly:

```bash
/path/to/shrike/design-partner/trial.sh /path/to/app origin/main
```

The script writes `.shrike/trial/delta.json` and `.shrike/trial/plan.json` and
prints a short summary. Exit code `0` means no orphans; `1` means findings were
reported; `4` means Shrike could not run and the reason is in the output.

## What the reports mean

- **Delta** — symbols and files that became unreachable because of this branch's
  change, compared with the merge base. Pre-existing dead code is not the
  headline.
- **Plan** — the bounded deletion batches Shrike would verify, plus blocked
  items. A plan is not a deletion. Nothing is applied in this trial.

## Trust boundary

- The Rust engine parses source and builds the graph. It does not execute
  project code.
- The host runs your configured checks only during `prune`, inside a worktree,
  after taking a baseline.
- The trial is read-only: no source edits, no commits, no network calls. It
  writes only under `.shrike/`.
- Build evidence is optional. If you provide a `next analyze export`, it can only
  add liveness, never authorize a deletion.
- A finding is a hypothesis. A receipt is the proof.

## Triage

Use [`triage-template.md`](triage-template.md) for every false positive or
blocked finding. A false positive is a corpus candidate; send the symbol, the
witness (or the reason it is wrong), and the smallest reproduction you can share.

## Removal

The trial writes only under `.shrike/`:

```bash
rm -rf .shrike
```

If you separately installed agent hooks, remove them with
`shrike hook uninstall --claude`, `--cursor`, or `--opencode`.

## Next step

When the read-only reports look useful, configure required checks and run
`shrike prune --base <ref>` on a branch. Shrike creates a branch and per-batch
commits, verifies each batch, and opens a draft PR when `gh` is available. It
never commits to the base branch.
