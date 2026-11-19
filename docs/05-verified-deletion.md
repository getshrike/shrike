# 05 — Verified Deletion

> **Parent:** [MASTER_PROMPT.md](../MASTER_PROMPT.md) · **Tracker:** [TRACKER.md](../TRACKER.md)
> **Status:** canonical · Authored 2026-11-20

---

## 1. Why verification is the product

The analyzer can be wrong. The parser can be wrong. The resolver can be wrong. A
dead-code fix is a code change. The only trustworthy test of a code change is
the project's own verification stack.

Shrike does not claim that a deletion is safe because its graph says so. It
claims that the deletion is safe **if**:

1. the analysis found no evidence of use under a conservative abstraction;
2. the deletion was applied in isolation;
3. the project still typechecks, lints, tests, and builds relative to a
   baseline; and
4. the result is recorded in a receipt that a human or CI can inspect.

The verifier is the third condition. The receipt is the fourth. Together they
turn a finding into a proof-carrying change.

This is the step that separates Shrike from read-only analyzers and advisory
safety tools.

---

## 2. The contract

> **No automated edit is considered successful until the project's configured
> checks pass against a baseline. A failed edit is reverted, never left
> half-applied.**

The contract has five stages:

```text
baseline → apply → check → compare → revert-or-keep
```

### 2.1 Baseline

A baseline captures the project's check results **before** the deletion:

- git HEAD, branch, dirty state;
- the exact commands, working directories, and environment;
- exit code, duration, status, and a failure signature for each check;
- flaky classification from reruns;
- the tool and runtime versions.

The baseline is content-addressed and stored in `.shrike/baseline.json` or in
the receipt directory. It is valid for one head commit and one check
configuration. A different head or a different check set requires a new
baseline.

### 2.2 Apply

The deletion is applied in one of two modes:

| Mode | Use | Behavior |
|---|---|---|
| `worktree` | Default for CI and shared repos | `git worktree add --detach`; apply; verify; remove |
| `in-place` | Local, clean tree, fast loop | Snapshot touched files; apply; verify; restore on failure |

`in-place` refuses a dirty tree unless `--allow-dirty` is explicit. The default
is `worktree`.

### 2.3 Check

Checks are detected from `package.json` scripts or supplied in
`shrike.config.ts`:

| Tier | Checks | Purpose |
|---|---|---|
| Fast | typecheck, lint | The hook path; seconds |
| Standard | typecheck, lint, test | The default local path |
| Full | typecheck, lint, test, build | The CI and PR path |

A check is a command, a working directory, a timeout, a required flag, and a
rerun count. The verifier never guesses a command from a script name if the
config supplies one.

### 2.4 Compare

Comparison is against the baseline, not against zero.

| After status | Baseline status | Classification |
|---|---|---|
| pass | pass | `pass` |
| pass | fail | `improvement` |
| fail | fail, same signature | `known-failure` |
| fail | fail, different signature | `new-failure` |
| fail | pass | `fail` |
| timeout | timeout, same signature | `known-failure` |
| timeout | pass | `timeout` |
| fail on rerun then pass | — | `flaky` |

`known-failure` does not block a deletion; it is pre-existing debt.
`new-failure`, `fail`, and `timeout` do.

### 2.5 Revert or keep

- All required checks pass or are known failures → keep the batch, write the
  receipt, continue.
- Any required check regresses → revert the batch, write the rejection, bisect
  if the batch has more than one independent unit.
- Revert itself fails → exit 5, preserve the recovery snapshot, and print the
  exact restore command.

The tree is never left half-applied.

---

## 3. Batch verification

A batch is a set of deletions that:

- forms a complete cascade closure (no symbol is left with a dead-only
  referrer);
- has no shared files with another batch;
- has no dependency ordering with another batch;
- is small enough to bisect in reasonable time (default: ≤ 50 symbols, ≤ 10
  files, ≤ 5% of the live graph).

Batches are verified independently. If batch 1 passes and batch 2 fails, batch 1
stays applied and batch 2 is reverted. This is the difference between "the tool
broke my app" and "one batch was rejected; the rest landed."

### 3.1 Bisection

When a batch fails:

1. Split the batch into halves by symbol count, respecting SCCs and the
   no-shared-file invariant.
2. Verify each half against the same baseline.
3. Recurse into the failing half.
4. Stop at a single SCC or a single symbol.
5. Revert the minimal failing set; record it with the check and signature.
6. Retry the remainder, excluding the culprit.

Bisection is deterministic: sort by `SymbolId` before splitting. The same
failure produces the same rejection set.

If both halves pass but the full batch fails, the failure is an **interaction
failure**. Do not attribute it to a single symbol; revert the whole batch and
record the interaction.

### 3.2 Flaky handling

- A failed check reruns up to `reruns` times (default 1 for tests, 0 otherwise).
- A pass after a failure is `flaky`, not `pass`.
- A flaky test does not block deletion, but the receipt records the rerun.
- A test that fails with a different signature on each run is `flaky` and is
  reported as an unstable check, not as a regression.

### 3.3 Failure signatures

A failure signature is a normalized hash of the check output:

- strip timings, durations, and temp paths;
- strip ANSI color;
- strip absolute paths;
- keep the error type, message, file, line, and test name;
- hash the result with BLAKE3.

The same pre-existing failure must produce the same signature across runs. If it
does not, the check is unstable and classified as `flaky`.

---

## 4. Receipt

A receipt is the durable proof of a verified deletion. It is written for every
batch, including rejected ones.

### 4.1 Schema

```json
{
  "schemaVersion": "1.0",
  "id": "rcpt_01J...",
  "createdAt": "2026-11-20T00:00:00.000Z",
  "tool": { "name": "shrike", "version": "1.0.0" },
  "base": { "ref": "origin/main", "commit": "abc123", "graphHash": "sha256:..." },
  "head": { "commit": "def456", "graphHash": "sha256:..." },
  "plan": { "id": "plan_01J...", "hash": "sha256:..." },
  "build": {
    "source": "next analyze export",
    "snapshotHash": "sha256:...",
    "version": "16.4.0",
    "status": "full"
  },
  "batch": {
    "id": "batch_1",
    "symbols": ["src/lib/format.ts#formatInvoice"],
    "files": ["src/lib/format.ts", "src/lib/currency.ts"],
    "linesRemoved": 412,
    "patches": [".shrike/patches/batch_1.patch"]
  },
  "checks": [
    {
      "name": "typecheck",
      "command": "pnpm typecheck",
      "status": "pass",
      "baselineStatus": "pass",
      "durationMs": 812,
      "reruns": 0
    }
  ],
  "outcome": "verified",
  "rejected": [],
  "environment": {
    "node": "v24.0.0",
    "packageManager": "pnpm@12.18.0",
    "platform": "linux"
  }
}
```

### 4.2 Semantics

- `outcome` is `verified`, `rejected`, `known-failure`, `flaky`, `error`,
  `revert-failed`, or `interaction-failure`.
- `rejected` lists the minimal failing sets from bisection, each with the check
  name and failure signature.
- `checks` records both the after status and the baseline status, so a reader
  can distinguish "passed" from "was already failing."
- `patches` points to the exact diff that was applied.
- `build` is optional; absence means static evidence only.
- `policy` records the entry policy and blast-radius cap.

### 4.3 Storage

- Local: `.shrike/receipts/<id>.json`.
- CI: uploaded as an artifact.
- PR: rendered as a Markdown comment with a link to the JSON artifact.

The receipt ID is derived from the base, head, plan, batch, and check
configuration. `createdAt` is metadata and does not affect the ID, so re-running
the same plan on the same head with the same check configuration reproduces the
ID. The serialized file is not byte-identical because it contains the timestamp.

### 4.4 Integrity

- The receipt is content-addressed: the ID is a hash of the canonical payload
  excluding `createdAt`.
- A downstream job can verify the hash.
- Optional signing is a later feature; the content hash is the first integrity
  mechanism.
- A receipt without a complete check record is never `verified`.

Full schema: [`../schemas/receipt.schema.json`](../schemas/receipt.schema.json).

---

## 5. Exit codes

| Code | Meaning | Caller action |
|---|---|---|
| 0 | Verified or clean | Continue |
| 1 | Findings or delta above threshold | Review |
| 2 | Configuration or usage error | Fix invocation |
| 3 | Verification failed; changes reverted | Inspect receipt |
| 4 | Verification could not run | Fix environment; fail closed |
| 5 | Revert failed | Stop; restore snapshot manually |

Exit 5 is the highest severity. It means the tool could not guarantee the tree's
state. The message must include the recovery path and the exact command to
restore.

---

## 6. Modes

### 6.1 Local fast path

`shrike hook` runs after an agent edit:

- compute the delta against the working tree's base;
- run typecheck and lint only;
- report candidate deletions and a fast-path receipt;
- do not apply.

This is feedback for the agent, not a deletion.

### 6.2 Local prune

`shrike prune --verify`:

- compute the plan;
- apply and verify each batch in a worktree;
- leave the verified changes in the tree or open a PR;
- write receipts.

### 6.3 CI

`shrike prune --ci`:

- run in a worktree;
- full checks;
- upload the receipt;
- open or update a PR;
- exit non-zero on regression.

### 6.4 Refused states

- dirty tree with `in-place` and no `--allow-dirty`;
- missing check command;
- engine unavailable or the verifier cannot start;
- config cannot load;
- baseline from a different head;
- worktree creation failure;
- a deletion that touches a file outside the plan;
- a batch over the blast-radius cap without an explicit override.

Refusal is a feature. It is how the tool keeps its promise.

---

## 7. What "verified" does not mean

- It does not mean the code is semantically unused in production. It means the
  project's configured checks passed after removal.
- It does not mean the tests are good. It means the tests that exist still pass.
- It does not mean the deletion is reversible forever. It means the patch and
  the receipt are retained.
- It does not mean the build is byte-identical. It means the build succeeded
  relative to the baseline.
- It does not mean the code is not used by an external consumer. Public API
  blocking and package boundaries handle that; the verifier is the second line
  of defense.

The receipt states exactly what was checked, so the claim is bounded and
auditable. Do not overclaim in the UI.

---

## 8. Failure modes and responses

| Failure | Response |
|---|---|
| Baseline check already fails | Record `known-failure`; do not block unless the signature changes |
| Test is flaky | Rerun; classify; record; do not block on flake alone |
| Build timeout | Fail the batch; do not retry indefinitely |
| Check writes to the repo | Run in a worktree; restore snapshots; report the side effect |
| Patch applies partially | Restore snapshots; exit 3; do not continue |
| Revert fails | Exit 5; preserve recovery snapshot; print restore command |
| Engine crashes | Exit 4; leave the worktree; print its path |
| Verifier crashes | Exit 4; leave the worktree; print its path |
| Receipt write fails | Treat the verification as failed; do not claim success |
| Bisection timeout | Revert the whole batch; exit 4; do not guess |
| Interaction failure | Revert the whole batch; record `interaction-failure` |
| Blast-radius refusal | Report the split/refusal; do not silently drop |

---

## 9. PR flow

```text
shrike prune --base origin/main --verify --receipt
  → plan
  → for each batch:
      apply in worktree
      baseline checks
      after checks
      compare
      keep or bisect/revert
      commit verified batch on a branch
      write receipt
  → open a draft PR
  → post the receipt as a comment
```

The PR body:

```markdown
## Verified cleanup

Shrike removed **1,240 lines** across **18 files** in **3 verified batches**.

| Batch | Symbols | Files | Lines | Checks |
|---|---|---|---|---|
| batch_1 | 22 | 6 | 812 | typecheck ✓ lint ✓ test ✓ build ✓ |
| batch_2 | 11 | 7 | 318 | typecheck ✓ lint ✓ test ✓ build ✓ |
| batch_3 | 4 | 5 | 110 | typecheck ✓ lint ✓ test ✓ build ✓ |

**Rejected and reverted:** 3 deletions (test failure in `invoice.test.ts`).
**Orphan delta:** 14 of these symbols were orphaned by this PR, not pre-existing.

Receipt: `.shrike/receipts/rcpt_01J....json`
```

The receipt is the artifact. The PR body is the human summary.

Review model:

- draft by default;
- one commit per batch;
- the patch for each batch is attached or linked;
- the receipt is immutable; a re-run produces a new receipt.

---

## 10. Acceptance criteria

- [ ] A deliberately broken deletion is detected and reverted; `git status` is
      clean.
- [ ] A batch containing one bad deletion is bisected to the culprit; the rest
      is kept.
- [ ] A pre-existing failing test is classified as `known-failure` and does not
      block.
- [ ] A flaky test is rerun, classified, and recorded.
- [ ] A missing check command exits 4 and deletes nothing.
- [ ] A failed revert exits 5 and prints a working restore command.
- [ ] A batch over the blast-radius cap is split or refused with a reason.
- [ ] An interaction failure reverts the whole batch and records it.
- [ ] Receipts validate against the schema and reproduce for the same input.
- [ ] The full suite runs on a real Next.js app with build + tests.
- [ ] The PR has one commit per verified batch.
- [ ] The tree is clean after every failure path.

---

## 11. Open questions

| ID | Question | Owner |
|---|---|---|
| Q-002 | Worktree by default? | PLAN-3: yes; in-place is explicit |
| Q-004 | Required vs advisory checks? | PLAN-3: typecheck/lint/test/build required by default; configurable |
| Q-005 | Sign receipts? | PLAN-3: content-address first; signing later |
| Q-026 | Affected-only test selection? | PLAN-3: start full; add selection after measuring |
| Q-036 | Persistent verifier process or one shot per batch? | PLAN-3: one shot per batch |
| Q-040 | How are monorepo checks scoped? | PLAN-3 |
| Q-041 | How are check side effects detected? | PLAN-3 |
