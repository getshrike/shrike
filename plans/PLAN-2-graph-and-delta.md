# PLAN-2 — Graph and Delta

> **Parent:** [MASTER_PROMPT.md](../MASTER_PROMPT.md) · **Tracker:** [TRACKER.md](../TRACKER.md)
> **Status:** `planned` · **Depends on:** PLAN-1 · **Unblocks:** PLAN-3
> **Owner prompts:** [`../prompts/01-rust-engine-agent.md`](../prompts/01-rust-engine-agent.md),
> [`../prompts/02-fixture-and-corpus-agent.md`](../prompts/02-fixture-and-corpus-agent.md)

---

## 1. Goal

Implement the whole-graph PR orphan delta: compare the merge-base snapshot to the
head snapshot, report newly dead symbols and files anywhere in the repository,
explain the lost witness, compute the cascade closure, and partition it into
independent deletion batches.

This is the plan that turns Shrike from "another analyzer" into "the tool that
finds what the PR broke."

---

## 2. Scope

### In scope

- Base snapshot acquisition at the merge base.
- Head snapshot acquisition at the PR head, including a dirty-tree mode.
- Comparability checks.
- Reachability diff and witness lookup.
- Trigger attribution.
- Tarjan SCC condensation over candidates.
- Cascade closure with exclusive ownership and stop conditions.
- Conflict graph, deterministic batch partitioning, blast radius.
- Impact estimation.
- `shrike delta` and `shrike plan`.
- Delta fixtures, monorepo handling, determinism, and a real-app run.

### Out of scope

- Applying deletions (PLAN-3).
- Verifying checks (PLAN-3).
- Writing receipts or opening PRs (PLAN-3).
- Build-graph evidence (PLAN-4).

---

## 3. Work breakdown

### 2.1 — Base snapshot acquisition

**Task:** Resolve the merge base and build a read-only snapshot from it.

**Acceptance:**

- `git merge-base <base> HEAD` is used, not the current tree.
- Order: cached snapshot → detached worktree → `git archive` → fail closed.
- The base snapshot is never mutated or verified.
- Stale worktrees under `.shrike/worktrees/` are cleaned.
- A missing `git`, a shallow clone without the base, or a failed worktree exits 4
  with a clear reason.

**Read:** [`../docs/04-whole-graph-delta.md`](../docs/04-whole-graph-delta.md) §3.1
and §9.

**Evidence:** fixture test with a real merge base.

### 2.2 — Head snapshot

**Task:** Build a snapshot from the PR head commit, or from the working tree for
local runs.

**Acceptance:**

- A clean head records the commit and `dirty: false`.
- A dirty head records `dirty: true` and the content hash of the working tree.
- A dirty head is allowed for local delta and hook output.
- A dirty head is refused for a CI receipt unless `--allow-dirty` is explicit.
- Untracked files under the include set are included; ignored files are not.

**Read:** [`../docs/04-whole-graph-delta.md`](../docs/04-whole-graph-delta.md) §3.2.

**Evidence:** fixture test for clean and dirty heads.

### 2.3 — Comparability gate

**Task:** Refuse to diff snapshots that were not built under the same inputs.

**Acceptance:**

- Compares config hash, resolver hash, entry adapter version, tool version,
  protocol version, and workspace layout.
- On mismatch, returns `incomparable` with a named reason and the changed input.
- Does not produce a misleading delta.
- Optionally offers a head-only full scan, clearly labeled as not a delta.

**Read:** [`../docs/04-whole-graph-delta.md`](../docs/04-whole-graph-delta.md) §3.3.

**Evidence:** unit tests for each mismatch.

### 2.4 — Live-set diff

**Task:** Compute `live(base) \ live(head)` over stable symbol keys.

**Acceptance:**

- The diff is by stable symbol key, not by array index.
- A renamed symbol is a removal plus an addition, not a silent match.
- Pre-existing dead code at base is excluded.
- A no-op PR produces an empty delta.
- A file deleted at head yields its exports as candidates if they were live at
  base.

**Read:** [`../docs/03-algorithms.md`](../docs/03-algorithms.md) §13.

**Evidence:** `delta-removed-caller`, `delta-renamed-export`, `delta-no-change`,
`delta-preexisting` fixtures.

### 2.5 — Witness lookup

**Task:** Retrieve the base witness for every delta symbol.

**Acceptance:**

- Every delta item has a lost witness or an explicit `no-witness` reason.
- The witness is the shortest path from an entry at base.
- The witness edges exist in the base snapshot.
- A witness is rendered as stable symbol keys, not numeric IDs.

**Read:** [`../docs/04-whole-graph-delta.md`](../docs/04-whole-graph-delta.md) §4.2.

**Evidence:** witness assertions in delta fixtures.

### 2.6 — Trigger attribution

**Task:** Diff base and head edges and attribute each orphan to the changed edge
that removed its last witness.

**Acceptance:**

- Walks the lost witness in order and finds the first missing or changed edge.
- Reports `removed-import`, `removed-call`, `removed-jsx`, `removed-route`,
  `renamed-export`, `removed-re-export`, `changed-dynamic`, or `indirect`.
- Names the changed file and symbol.
- Does not claim intent.
- If no single edge explains it, reports `indirect` with the closest changed
  ancestor.

**Read:** [`../docs/04-whole-graph-delta.md`](../docs/04-whole-graph-delta.md) §6.

**Evidence:** triggers asserted in every delta fixture.

### 2.7 — Cascade closure

**Task:** Compute the closure of exclusive dependencies over the SCC condensation
DAG.

**Acceptance:**

- Only complete SCCs enter the closure.
- A dependency joins only when every live referrer is already in the closure.
- Stop at an entry, public API, external live referrer, unknown incoming edge,
  probable external edge, unparsed file, generated file, test-only entry under a
  policy that excludes it, or the blast-radius policy.
- The closure is deterministic.
- A test proves every closure symbol has all live referrers inside the closure.

**Read:** [`../docs/03-algorithms.md`](../docs/03-algorithms.md) §15–§16.

**Evidence:** `delta-cycle`, `delta-barrel`, `delta-dynamic`,
`delta-public-api` fixtures.

### 2.8 — Batch conflict graph

**Task:** Build a conflict graph over deletion units (SCCs) and partition with
deterministic greedy coloring.

**Acceptance:**

- Units conflict if they share a file, have a predecessor relationship, share a
  barrel re-export path, or cross a public API/generated boundary.
- Units are sorted by impact desc, then minimum `SymbolId`.
- No batch shares a file with another batch.
- No batch splits an SCC.
- Batch IDs and order are deterministic across runs.

**Read:** [`../docs/03-algorithms.md`](../docs/03-algorithms.md) §18.

**Evidence:** unit tests; a fixture with two independent subtrees.

### 2.9 — Blast-radius guard

**Task:** Enforce the batch cap and report a split or refusal.

**Acceptance:**

- Defaults: ≤ 50 symbols, ≤ 10 files, ≤ 5% of the live graph.
- An oversized color is split by file groups, then by sorted unit IDs.
- An oversized single SCC is refused unless `--allow-oversized`.
- A refusal is reported, not silently dropped.
- An override is recorded in the plan and later in the receipt.

**Read:** [`../docs/03-algorithms.md`](../docs/03-algorithms.md) §19.

**Evidence:** unit tests for split and refuse.

### 2.10 — Impact estimation

**Task:** Estimate symbols, files, removed files, lines, routes, and bytes.

**Acceptance:**

- Line counts merge overlapping spans per file; no double counting.
- `filesRemoved` counts only files with no live content remaining.
- Routes come from base witnesses; bytes are absent until PLAN-4.
- The plan reports the estimation method.
- The receipt later records exact patch stats.

**Read:** [`../docs/03-algorithms.md`](../docs/03-algorithms.md) §17.

**Evidence:** impact assertions in fixtures.

### 2.11 — `shrike delta`

**Task:** Implement the CLI command with human and JSON output.

**Acceptance:**

- `shrike delta --base origin/main` prints the summary and the top findings.
- `--json` validates against `schemas/delta.schema.json`.
- Exit 0 for an empty delta; 1 for findings above the threshold; 2 for usage;
  4 for an engine or comparability failure.
- The human output includes the lost witness and trigger for each finding.
- The command is read-only.

**Read:** [`../docs/04-whole-graph-delta.md`](../docs/04-whole-graph-delta.md) §7.

**Evidence:** CLI integration test; schema validation.

### 2.12 — `shrike plan`

**Task:** Implement the deletion plan command.

**Acceptance:**

- Prints batches with symbol/file/line counts and blast-radius decisions.
- `--json` validates against `schemas/deletion-plan.schema.json`.
- An empty plan is valid.
- A refused or split batch is visible.
- The plan includes the base/head hashes and the policy.

**Read:** [`../docs/03-algorithms.md`](../docs/03-algorithms.md) §18–§19.

**Evidence:** CLI integration test; schema validation.

### 2.13 — Delta fixtures and corpus

**Task:** Implement the nine delta scenarios plus generated, public API,
test-policy, interaction, and dirty-tree fixtures.

**Acceptance:**

- Every fixture has a README, base tree, head tree, and `expected.json`.
- The must-not-report corpus produces zero delta findings.
- `delta-interaction` attributes an indirect trigger.
- `delta-dirty` is marked and refused for CI receipts.
- The runner reports a readable diff on failure.

**Read:** [`../docs/04-whole-graph-delta.md`](../docs/04-whole-graph-delta.md) §11.

**Evidence:** fixture suite report.

### 2.14 — Monorepo handling

**Task:** Represent workspace boundaries and resolve package imports across them.

**Acceptance:**

- pnpm, npm, and yarn workspaces are detected.
- A workspace package's public entry is an entry; its internals are not
  automatically public.
- A deletion does not cross a package boundary without an explicit allow.
- A shared package used by two apps keeps its exports live.
- A fixture covers six packages with a shared UI package.

**Read:** [`../docs/03-algorithms.md`](../docs/03-algorithms.md) §7 and §9.4.

**Evidence:** `fixtures/monorepo/`; `delta-monorepo` fixture.

### 2.15 — Determinism

**Task:** Prove byte-identical delta and plan output across runs and thread
counts.

**Acceptance:**

- Two runs produce identical JSON.
- 1, 2, and 8 threads produce identical JSON.
- A different filesystem order produces identical JSON.
- No timestamps or random IDs appear in the delta or plan payload.

**Read:** [`../docs/03-algorithms.md`](../docs/03-algorithms.md) §24.

**Evidence:** CI log.

### 2.16 — Real-app delta

**Task:** Run the delta on a pinned real Next.js app with a synthetic feature
removal.

**Acceptance:**

- The synthetic removal orphans at least one symbol in an untouched file.
- The lost witness and trigger are correct.
- The cascade closure is complete.
- No pre-existing dead code appears in the delta.
- The run is recorded with the commit and command.

**Read:** [`../docs/04-whole-graph-delta.md`](../docs/04-whole-graph-delta.md) §1.

**Evidence:** report in the tracker.

### 2.17 — Docs update

**Task:** Update `docs/03` and `docs/04` to match the implemented behavior.

**Acceptance:**

- Every deviation is either fixed or documented with a decision.
- The open questions for PLAN-2 are resolved or reassigned.
- The tracker links the updated docs.

**Evidence:** review.

---

## 4. Exit gate

- [ ] The nine delta fixtures pass, plus the five added scenarios.
- [ ] The must-not-report corpus is green in delta mode.
- [ ] A removed caller in a touched file reports the untouched orphan.
- [ ] Every delta item has a lost witness and a trigger.
- [ ] No partial SCC appears in a plan.
- [ ] No batch shares a file with another batch.
- [ ] The blast-radius guard splits or refuses with a reason.
- [ ] A monorepo fixture respects package boundaries.
- [ ] Output is byte-identical across runs and thread counts.
- [ ] A real-app synthetic removal produces the expected untouched-file orphan.
- [ ] `shrike delta --json` and `shrike plan --json` validate against the
      schemas.

---

## 5. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Noisy delta from generated code | Generated-file policy; report exclusions |
| Unstable IDs across commits | Keys, not indices; rename detection via span + kind + name |
| Trigger misattribution | Walk the witness; prefer `indirect` over guessing |
| Oversized closure | Blast-radius split/refuse; explicit override |
| Monorepo false positives | Workspace entry model; cross-package guard |
| Case-only renames on case-insensitive filesystems | Normalize and test both cases |
| Shallow clone lacks merge base | Fail closed with a fetch hint |

---

## 6. Definition of done

A developer can run `shrike delta --base origin/main` on a real PR and see the
code the PR orphaned in files it never touched, with the lost witness and the
triggering change, partitioned into safe batches — and the output is the same
every time.
