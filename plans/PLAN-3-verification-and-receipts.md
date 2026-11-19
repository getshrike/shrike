# PLAN-3 — Verification and Receipts

> **Parent:** [MASTER_PROMPT.md](../MASTER_PROMPT.md) · **Tracker:** [TRACKER.md](../TRACKER.md)
> **Status:** `planned` · **Depends on:** PLAN-1, PLAN-2 · **Unblocks:** PLAN-4, PLAN-5
> **Owner prompt:** [`../prompts/03-verifier-agent.md`](../prompts/03-verifier-agent.md)

---

## 1. Goal

Deliver the product sentence end to end:

> Shrike deletes the code that agents and PRs leave behind — and proves the app
> still builds.

Concretely: take a deletion plan, apply each batch in isolation, run the
project's checks against a baseline, bisect failures, revert only the culprit,
write a receipt, and open a draft PR.

This is the plan where Shrike becomes a product rather than an analyzer.

---

## 2. Scope

### In scope

- `packages/verifier`: baseline, check discovery, worktree/in-place apply,
  comparison, signatures, flaky handling, bisection, revert.
- `packages/receipts`: writer, validator, content hash, PR renderer.
- `packages/cli` integration: `prune`, `verify`, `receipt`.
- Exit codes 0–5.
- A failure drill and a real-app end-to-end run.
- The first draft PR with per-batch commits and a receipt comment.

### Out of scope

- Build-graph evidence (PLAN-4).
- Hooks, Action, MCP (PLAN-5).
- Signing receipts (later; content hash first).
- Affected-only test selection (start full, measure, then optimize).

---

## 3. Work breakdown

### 3.1 — Verifier package scaffold

**Task:** Create `packages/verifier` with a typed API: `captureBaseline`,
`runChecks`, `applyBatch`, `revert`, `compare`, `bisect`.

**Acceptance:**

- No CLI argument parsing inside the package.
- Every function returns a typed result or a typed error.
- No `process.exit` in the package.
- Unit tests cover the API surface and error paths.
- The package has no dependency on `packages/cli`.

**Read:** [`../docs/05-verified-deletion.md`](../docs/05-verified-deletion.md) §2.

**Evidence:** unit tests.

### 3.2 — Baseline capture

**Task:** Capture the project's check results before any deletion.

**Acceptance:**

- Records HEAD, branch, dirty state, commands, cwd, environment, exit codes,
  durations, statuses, and failure signatures.
- Records tool and runtime versions.
- The baseline is content-addressed and tied to one head and one check set.
- A different head or check set invalidates the baseline with a clear error.
- A baseline write failure fails closed.

**Read:** [`../docs/05-verified-deletion.md`](../docs/05-verified-deletion.md) §2.1.

**Evidence:** baseline fixture test.

### 3.3 — Check discovery

**Task:** Discover checks from `package.json` scripts and `shrike.config.ts`.

**Acceptance:**

- Explicit config commands win over script-name inference.
- Recognizes `typecheck`/`type-check`, `lint`, `test`, and `build`.
- A missing required check is an error (exit 4), not a skip.
- A check records command, cwd, timeout, required, and reruns.
- Interactive/watch scripts are rejected.
- A check that is `prettier --check` is not treated as a typecheck.

**Read:** [`../docs/05-verified-deletion.md`](../docs/05-verified-deletion.md) §2.3.

**Evidence:** unit tests for each discovery case.

### 3.4 — Worktree mode

**Task:** Implement the default apply mode with `git worktree add --detach`.

**Acceptance:**

- Requires a clean tree.
- Creates the worktree under `.shrike/worktrees/<run>`.
- Applies the batch, runs checks, and removes the worktree on success.
- On handled failure, removes the worktree after revert.
- On an unhandled crash, leaves the worktree and prints its path.
- Stale worktrees are cleaned by the next run.
- The main tree is never touched.

**Read:** [`../docs/05-verified-deletion.md`](../docs/05-verified-deletion.md) §2.2.

**Evidence:** integration test with a real git repository.

### 3.5 — In-place mode

**Task:** Implement the explicit local mode with snapshots.

**Acceptance:**

- Refuses a dirty tree unless `--allow-dirty` is explicit.
- Snapshots every touched file before apply.
- Restores on any failure.
- Verifies the restored tree byte-for-byte.
- A restore failure exits 5 and prints the snapshot path and restore command.
- The mode is never the default.

**Read:** [`../docs/05-verified-deletion.md`](../docs/05-verified-deletion.md) §2.2 and §6.4.

**Evidence:** integration test, including a forced restore failure.

### 3.6 — Compare and classification

**Task:** Compare after-check results to the baseline.

**Acceptance:**

- Implements `pass`, `improvement`, `known-failure`, `new-failure`, `fail`,
  `timeout`, and `flaky`.
- `known-failure` requires the same signature, not just the same exit code.
- `new-failure` is a regression and blocks the batch.
- A timeout against a passing baseline is `timeout` and blocks.
- The comparison is recorded in the receipt.

**Read:** [`../docs/05-verified-deletion.md`](../docs/05-verified-deletion.md) §2.4.

**Evidence:** unit tests for every row of the table.

### 3.7 — Batch apply

**Task:** Apply each batch independently and verify it.

**Acceptance:**

- Batches are verified in deterministic order.
- A verified batch is kept; a failed batch is reverted.
- A later batch starts from the kept state of earlier batches.
- A batch never shares a file with another batch.
- A partial apply is detected and fails closed.
- The batch patch is stored under `.shrike/patches/`.

**Read:** [`../docs/03-algorithms.md`](../docs/03-algorithms.md) §20 and
[`../docs/05-verified-deletion.md`](../docs/05-verified-deletion.md) §3.

**Evidence:** integration test with three batches.

### 3.8 — Bisection

**Task:** Isolate a failing deletion inside a batch.

**Acceptance:**

- Splits by sorted SCC units, respecting the no-shared-file invariant.
- Recurses into the failing half.
- Stops at a single SCC or symbol.
- Reverts the minimal failing set and records the check and signature.
- Retries the remainder, excluding the culprit.
- Both halves pass but the full batch fails → `interaction-failure`, whole batch
  reverted.
- A bisection timeout reverts the whole batch and exits 4.
- The same failure produces the same rejection set.

**Read:** [`../docs/03-algorithms.md`](../docs/03-algorithms.md) §22.

**Evidence:** failure drill; unit tests for interaction and timeout.

### 3.9 — Revert and recovery

**Task:** Implement snapshot restore and failure recovery.

**Acceptance:**

- A successful revert restores every touched file and leaves `git status` clean.
- A failed revert exits 5, preserves the recovery snapshot, and prints an exact
  restore command.
- The receipt records `revert-failed`.
- No path continues after a revert failure.
- A recovery snapshot is readable by a documented `shrike recover` command.

**Read:** [`../docs/05-verified-deletion.md`](../docs/05-verified-deletion.md) §2.5.

**Evidence:** forced-failure test.

### 3.10 — Receipt writer

**Task:** Write a content-addressed receipt for every batch, including rejected
ones.

**Acceptance:**

- Validates against `schemas/receipt.schema.json`.
- Includes base/head/plan hashes, batch, patches, checks, baseline status,
  outcome, rejected sets, policy, environment, and build status (absent until
  PLAN-4).
- The ID is a hash excluding `createdAt`.
- Re-running the same plan on the same head reproduces the ID.
- A receipt write failure makes the verification fail.
- A `verified` receipt has no rejected set and at least one check.

**Read:** [`../docs/05-verified-deletion.md`](../docs/05-verified-deletion.md) §4.

**Evidence:** schema tests; reproducibility test.

### 3.11 — Receipt validation

**Task:** Validate every receipt produced by the fixtures and the real-app run.

**Acceptance:**

- A schema validation failure fails CI.
- A receipt with a missing check field fails.
- A receipt with a hash mismatch fails.
- A rejected receipt validates with an empty `rejected` only when the outcome is
  not `rejected`.
- The validator runs in `scripts/validate-schemas.mjs`.

**Evidence:** CI log.

### 3.12 — PR renderer

**Task:** Render the PR body and receipt comment.

**Acceptance:**

- Markdown tables are escaped and truncated.
- The body includes batch counts, lines, files, check status, rejected sets, and
  the orphan-delta count.
- The receipt comment links the JSON artifact and shows the content hash.
- The renderer never includes raw check logs beyond a truncated, redacted
  excerpt.
- Snapshot tests cover a fully verified PR and a partially rejected PR.

**Read:** [`../docs/05-verified-deletion.md`](../docs/05-verified-deletion.md) §9.

**Evidence:** snapshot tests.

### 3.13 — Per-batch commits and draft PR

**Task:** Create a branch, commit each verified batch, and open a draft PR.

**Acceptance:**

- Branch `shrike/cleanup-<shortsha>`.
- One commit per verified batch; message names the batch and symbol count.
- Draft PR by default.
- Receipt comment posted after the PR is created.
- A diverged branch stops the flow and reports; no force-push.
- Never commits to `main`.
- The PR body and commits are reproducible from the plan and receipts.

**Read:** [`../docs/07-agent-and-distribution.md`](../docs/07-agent-and-distribution.md) §3.

**Evidence:** integration test on a test repository.

### 3.14 — Exit codes

**Task:** Implement and test exit codes 0–5.

**Acceptance:**

- 0: verified or clean.
- 1: findings or delta above threshold.
- 2: configuration or usage error.
- 3: verification failed; changes reverted.
- 4: verification could not run (missing engine, missing check, timeout).
- 5: revert failed.
- Every path is covered by a CLI test.

**Read:** [`../docs/05-verified-deletion.md`](../docs/05-verified-deletion.md) §5.

**Evidence:** CLI test matrix.

### 3.15 — Flaky handling

**Task:** Rerun failed checks and classify flakiness.

**Acceptance:**

- Test checks rerun once by default; other checks do not.
- A pass after a failure is `flaky`, not `pass`.
- A flaky check does not block a batch but is recorded.
- A check with a different signature on each run is unstable and recorded.
- A known-failure that becomes flaky is not treated as a regression.

**Read:** [`../docs/05-verified-deletion.md`](../docs/05-verified-deletion.md) §3.2–§3.3.

**Evidence:** flaky fixture; unit tests.

### 3.16 — Failure drill

**Task:** Add a CI drill that applies a deliberately broken deletion.

**Acceptance:**

- The broken deletion is detected, bisected to the culprit, and reverted.
- `git status` is clean afterward.
- The receipt records the rejected set and the check signature.
- The rest of the batch is kept if it verifies.
- The drill runs on every PR.

**Read:** [`../docs/05-verified-deletion.md`](../docs/05-verified-deletion.md) §10.

**Evidence:** CI log.

### 3.17 — Real-app end-to-end

**Task:** Run the full loop on a real Next.js app with typecheck, lint, test, and
build.

**Acceptance:**

- At least three batches verify.
- The PR is a draft with one commit per batch.
- The receipt validates and is posted as a comment.
- The orphan delta identifies at least one symbol in an untouched file.
- The full run is recorded with the commit and environment.

**Evidence:** receipt and PR link in the tracker.

### 3.18 — Docs update

**Task:** Update `docs/05` and `docs/07` to match the implemented behavior.

**Acceptance:**

- Every deviation is fixed or documented with a decision.
- The open questions for PLAN-3 are resolved or reassigned.
- The tracker links the updated docs.

**Evidence:** review.

---

## 4. Exit gate

- [ ] A deliberately broken deletion is detected and reverted; the tree is clean.
- [ ] A batch with one bad deletion is bisected; the rest is kept.
- [ ] A pre-existing failure is `known-failure` and does not block.
- [ ] A flaky check is rerun, classified, and recorded.
- [ ] A missing check exits 4 and deletes nothing.
- [ ] A failed revert exits 5 with a working restore command.
- [ ] An oversized batch is split or refused with a reason.
- [ ] An interaction failure reverts the whole batch and records it.
- [ ] Every receipt validates and reproduces for the same input.
- [ ] The real-app run produces a draft PR with per-batch commits.
- [ ] The receipt comment is posted and the JSON artifact is linked.
- [ ] The full path completes within 10 minutes on the reference app (or the
      miss is recorded).

---

## 5. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Check cost multiplies with batches | Batch cap; tiered checks; baseline cache |
| Flaky tests misclassify regressions | Reruns; signatures; `flaky` classification |
| Revert leaves a dirty tree | Snapshot + byte-for-byte verify; exit 5; drill |
| PR branch diverges | Stop; never force-push; report |
| A check writes outside the worktree | Worktree isolation; side-effect detection; restore |
| Receipt drifts from schema | Schema validation in CI; single writer |
| `gh` unavailable in CI | Fail closed for PR creation; keep the branch and receipt locally |
| Build timeouts | Explicit timeout; `timeout` classification; no infinite retry |

---

## 6. Definition of done

A user can run one command on a real repository and receive a draft PR containing
independent, verified deletion batches and a receipt that a reviewer can audit.
When a batch breaks the build, Shrike finds the culprit, reverts only it, keeps
the rest, and leaves the tree clean.
