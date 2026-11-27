# Workflows

Choose the workflow from the user's intent, not from the command they happen to
mention. Every workflow starts read-only unless the user explicitly asked for a
mutation.

## Decision tree

```text
What is dead in the current tree?          -> report or scan
What did this branch orphan?               -> report --base or delta --base
Show me the cleanup plan.                  -> plan --base
Clean this branch.                         -> clean --base (after a preview)
Clean the whole tree.                      -> clean --all only on explicit request
Why is this symbol unused?                 -> why or proof
Is this dependency unused?                 -> deps, advisory
Prove this cleanup.                        -> receipt, then verify
The cleanup failed with exit 5.            -> stop and recover the snapshot
Review a PR for dead code.                 -> delta --base, read-only
Set up CI.                                 -> init if needed, then ci install
Set up the agent skill or hook.            -> install or hook install
Use Shrike from MCP.                       -> mcp serve, read-only tools
```

## Branch cleanup

Goal: remove only what this branch orphaned, with checks and receipts.

```bash
npx getshrike report --base origin/main
npx getshrike delta --base origin/main
npx getshrike plan --base origin/main
npx getshrike clean --base origin/main --dry-run
npx getshrike clean --base origin/main
npx getshrike receipt
```

What each step proves:

1. `report --base` measures dead weight and separates branch-orphaned code from
   pre-existing dead code.
2. `delta --base` lists the newly dead symbols and files and names the removed
   edge for each one.
3. `plan --base` partitions the delta into independent batches and records the
   blast-radius policy. Blocked items appear here with a reason.
4. `clean --dry-run` shows the same batches in the apply order. It writes no
   project source and exits `1` if there are batches.
5. `clean` creates a branch worktree, captures a baseline, applies one batch at
   a time, runs the checks, bisects and reverts a regression, commits passing
   batches, and writes a receipt per batch. It never commits to the base branch.
6. `receipt` confirms the outcome. A verified batch has a content-addressed
   receipt that can be re-checked later.

Report the result as counts and the next command, for example: "Verified 2
batches (14 symbols, 3 files); 1 item was protected as public API. Receipt
`rcpt_<id>`; re-check with `verify --plan <id>`."

## Current-state cleanup

`clean --all` is not a bigger branch cleanup. It plans every removable file and
symbol in the current tree, so the blast radius is larger and the base is the
current commit.

Use it only when the user explicitly asks to clean the whole tree. Ask before
proceeding if the request is ambiguous.

```bash
npx getshrike scan
npx getshrike clean --all --dry-run
npx getshrike clean --all
npx getshrike receipt
```

`clean --all` rejects `--base`. If the user asks for both, explain the choice
and use branch cleanup for branch-orphaned code.

## PR review

A review is read-only. Do not clean, commit, or open a PR unless the user asks
for that separately.

```bash
npx getshrike delta --base origin/main
npx getshrike plan --base origin/main
npx getshrike why src/format.ts#formatInvoice
```

Summarize newly dead symbols, files, and the changed edge that removed the last
witness. Call out protected items and diagnostics separately. A diagnostic is
not by itself proof that code is dead.

If the review asks for a CI policy, point at `ci install` and `delta --fail-on`.
Do not apply a cleanup inside the review.

## CI

Advisory by default:

```bash
npx getshrike init
npx getshrike ci install
```

The generated workflow runs the Action in `mode: delta` with `fail-on: never`
and `contents: read`. Add `--gate` only with explicit consent; it makes newly
dead code block the pull request and adds `pull-requests: write` for the PR
comment.

To gate an existing pipeline without the generated workflow, run:

```bash
npx getshrike delta --base origin/main
```

Treat exit `1` as findings, `2` as a configuration problem, and `3`, `4`, or `5`
as tool failures. Use `delta --fail-on never` when the report should stay
advisory.

## Post-agent hook

The hook runs after an agent stops. It never deletes.

```bash
npx getshrike hook run
```

Read `.shrike/brief.json` (bounded to 20 items and 32 KB) or the human summary.
Then decide whether to run the branch workflow. The hook
degrades to delta-only on timeout and exits `0`; a failed check never blocks the
agent.

Install the hook for one client with `hook install --claude`, `--cursor`, or
`--opencode`. `install` also installs hooks for detected targets unless
`--no-hook` is passed.

## MCP

`mcp serve` exposes four read-only tools over stdio:

| Tool | Argument | Returns |
| --- | --- | --- |
| `shrike_delta` | `base` optional | Whole-graph PR orphan delta |
| `shrike_plan` | `base` optional | Deletion batches and blast radius |
| `shrike_proof` | `symbol` required | Witness path or proof trace |
| `shrike_receipt` | `id` optional | One receipt or the receipt list |

Use MCP for read-only inspection when the client already speaks MCP. No tool
applies deletions or runs project checks. `shrike_plan` writes plan artifacts
under `.shrike/`; all tools may write engine cache snapshots.

## Recovery

Exit `5` means the revert itself failed. The tree may be inconsistent.

1. Stop the cleanup. Do not continue, and do not hand-edit the worktree.
2. Read the snapshot path from the failing command's output.
3. Run `npx getshrike recover <snapshot>`.
4. Confirm `git status` and the relevant files.
5. Re-run the read-only report and re-plan before trying cleanup again.

Exit `3` is different: Shrike reverted the batch successfully. Read the receipt,
fix the project or the check, and re-plan. Do not bypass checks.

Exit `4` means Shrike could not produce a trustworthy result. Fix the stated
cause (missing engine, unresolved base, no checks, stale plan, or installer
failure) and re-run the same command.

## Setup

For a new repository:

```bash
npx getshrike init
npx getshrike install --dry-run
npx getshrike install
npx getshrike report
```

`init` writes `shrike.config.ts` and adds `.shrike/` to `.gitignore`. `install`
writes the portable skill and, for detected targets, adapters and hooks. Run
`report` to confirm the analysis works before configuring CI.
