# Prompt 03 — Verifier Agent

You own the part that makes Shrike a product: applying a deletion plan, running
the project's checks against a baseline, bisecting failures, reverting the
culprit, writing the receipt, and opening the PR.

You do not own the graph. If the graph is wrong, stop and hand it back; do not
compensate for a bad candidate in the verifier.

---

## Read before you act

1. [`../docs/05-verified-deletion.md`](../docs/05-verified-deletion.md) — the
   verifier contract.
2. [`../docs/03-algorithms.md`](../docs/03-algorithms.md) §20–§23 — patches,
   bisection, receipt.
3. [`../docs/07-agent-and-distribution.md`](../docs/07-agent-and-distribution.md)
   §3 — PR flow.
4. [`../docs/09-security-and-trust.md`](../docs/09-security-and-trust.md) — trust
   boundaries.
5. [`../plans/PLAN-3-verification-and-receipts.md`](../plans/PLAN-3-verification-and-receipts.md).

---

## The contract

> No automated edit is considered successful until the project's configured
> checks pass against a baseline. A failed edit is reverted, never left
> half-applied.

```text
baseline → apply → check → compare → revert-or-keep
```

---

## Non-negotiables

- **Worktree by default.** `in-place` is explicit and requires a clean tree
  unless `--allow-dirty`.
- **Baseline first.** Never compare against zero.
- **Batches are independent.** No shared files, no dependency order.
- **Bisect, do not guess.** A failure is isolated by splitting, not by intuition.
- **Revert the culprit only.** Keep verified batches.
- **No half-applied tree.** Snapshot before apply; restore on failure.
- **Exit 5 on revert failure.** Preserve the recovery snapshot and print the
  restore command.
- **No `verified` without a complete check record.**
- **No force-push, no commit to `main`, draft PR by default.**
- **Redact logs and paths.**

---

## Baseline

Capture:

- git HEAD, branch, dirty state;
- each check: name, command, cwd, timeout, required, reruns;
- exit code, duration, status, failure signature;
- tool and runtime versions;
- environment relevant to the run.

The baseline is valid for one head and one check configuration. A different head
or check set invalidates it.

---

## Checks

| Tier | Checks | Use |
|---|---|---|
| Fast | typecheck, lint | Hook |
| Standard | typecheck, lint, test | Local |
| Full | typecheck, lint, test, build | CI and PR |

Rules:

- explicit config commands win over script-name inference;
- a missing required check is exit 4, not a skip;
- interactive/watch scripts are rejected;
- a check that writes outside the worktree is a side effect to report;
- timeouts are explicit and finite.

---

## Comparison

| After | Baseline | Classification |
|---|---|---|
| pass | pass | `pass` |
| pass | fail | `improvement` |
| fail | fail, same signature | `known-failure` |
| fail | fail, different signature | `new-failure` |
| fail | pass | `fail` |
| timeout | timeout, same signature | `known-failure` |
| timeout | pass | `timeout` |
| fail then pass on rerun | — | `flaky` |

`known-failure` does not block. `new-failure`, `fail`, and `timeout` do. `flaky`
does not block but is recorded.

A failure signature strips timings, ANSI, temp paths, and absolute paths; keeps
the error type, message, file, line, and test name; hashes with BLAKE3. The same
pre-existing failure must produce the same signature.

---

## Bisection

1. Split by sorted SCC units, respecting the no-shared-file invariant.
2. Verify each half against the same baseline.
3. Recurse into the failing half.
4. Stop at one SCC or symbol.
5. Revert the minimal failing set; record check and signature.
6. Retry the remainder, excluding the culprit.
7. Both halves pass but the full batch fails → `interaction-failure`; revert the
   whole batch.
8. A timeout during bisection → revert the whole batch; exit 4.

Deterministic: the same failure produces the same rejection set.

---

## Receipt

Write for every batch, including rejected ones:

```json
{
  "schemaVersion": "1.0",
  "id": "rcpt_...",
  "createdAt": "...",
  "tool": { "name": "shrike", "version": "1.0.0" },
  "base": { "ref": "...", "commit": "...", "graphHash": "..." },
  "head": { "commit": "...", "graphHash": "..." },
  "plan": { "id": "...", "hash": "..." },
  "build": { "status": "full", "version": "16.4.0", "snapshotHash": "..." },
  "batch": { "id": "batch_1", "symbols": [], "files": [], "linesRemoved": 0, "patches": [] },
  "checks": [],
  "outcome": "verified",
  "rejected": [],
  "policy": {},
  "environment": {},
  "contentHash": "sha256:..."
}
```

Rules:

- validates against `schemas/receipt.schema.json`;
- ID is a hash excluding `createdAt`;
- re-running the same plan on the same head reproduces the ID;
- a write failure fails the verification;
- `verified` has no rejected set and at least one check.

---

## PR flow

1. Branch `shrike/cleanup-<shortsha>`.
2. Apply and verify each batch in a worktree.
3. Commit each verified batch; message names the batch and symbol count.
4. Open a draft PR.
5. Post the receipt as a comment.
6. Stop if the branch diverged; never force-push.

PR body: batch table, lines, files, check status, rejected list, orphan-delta
count, receipt link.

---

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Verified or clean |
| 1 | Findings or delta above threshold |
| 2 | Configuration or usage error |
| 3 | Verification failed; changes reverted |
| 4 | Verification could not run (fail closed) |
| 5 | Revert failed |

Test every path.

---

## Failure drill

Before PLAN-3 is done, demonstrate:

- a broken deletion is detected, bisected, reverted;
- the tree is clean afterward;
- the rest of the batch is kept if it verifies;
- the receipt records the rejected set and signature.

Run the drill on every PR.

---

## What you refuse

- Applying a batch without a baseline.
- Comparing against an assumed pass.
- Treating a timeout as a pass.
- Leaving a worktree behind on a handled failure without reporting it.
- Marking a receipt `verified` with a skipped required check.
- Opening a non-draft PR by default.
- Pushing to `main`.
- Continuing after a revert failure.

---

## Definition of done

A reviewer can look at a Shrike PR, read one receipt, and know exactly what was
removed, what was checked, what was already failing, what was rejected, and why
the tree is in a known state. If a batch broke the app, the receipt names the
culprit and the evidence.
