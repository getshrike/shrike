# PLAN-5 — Distribution and Launch

> **Parent:** [MASTER_PROMPT.md](../MASTER_PROMPT.md) · **Tracker:** [TRACKER.md](../TRACKER.md)
> **Status:** `planned` · **Depends on:** PLAN-3, PLAN-4 · **Unblocks:** —
> **Owner prompts:** [`../prompts/06-release-agent.md`](../prompts/06-release-agent.md),
> [`../prompts/07-design-partner-triage-agent.md`](../prompts/07-design-partner-triage-agent.md)

---

## 1. Goal

Put Shrike where the work happens — after the agent, in the PR, in CI — and prove
the positioning with real teams.

Distribution is not a marketing afterthought. It is a product surface. The
research is clear: the tools that grew in the AI-coding era met the agent in its
loop. `aislop` reached roughly 110k npm downloads per month with hooks, CI, MCP,
and a score. Shrike copies the distribution and rejects the score.

---

## 2. Scope

### In scope

- Claude Code, Cursor, and OpenCode post-agent hooks.
- GitHub Action and the draft-PR flow.
- PR comment with the receipt.
- MCP wrapper (four tools).
- Landing-page positioning test.
- Launch content: a 10-repo teardown, before/after, receipt screenshots.
- Design-partner program (3 teams).
- Package naming and release checklist.

### Out of scope

- A hosted dashboard.
- A paid plan or billing.
- A plugin marketplace.
- Multi-language support.

---

## 3. Work breakdown

### 5.1 — Claude Code hook

**Task:** Install a `Stop`/`SubagentStop` hook that runs the fast path.

**Acceptance:**

- `shrike hook install --claude` writes an idempotent hook block.
- `shrike hook uninstall --claude` removes only that block.
- The hook runs the delta and fast checks within 60 s or degrades.
- It never applies a deletion.
- It writes only under `.shrike/`.
- A missing engine prints one line and exits 0.

**Read:** [`../docs/07-agent-and-distribution.md`](../docs/07-agent-and-distribution.md) §2.

**Evidence:** integration test; manual run in Claude Code.

### 5.2 — Cursor hook

**Task:** Install the Cursor equivalent.

**Acceptance:**

- Install/uninstall is idempotent.
- The event is task-end or a debounced edit, documented in the config comment.
- The same fast path and budget apply.
- The output is a one-screen summary plus JSON.

**Read:** [`../docs/07-agent-and-distribution.md`](../docs/07-agent-and-distribution.md) §2.3.

**Evidence:** integration test; manual run in Cursor.

### 5.3 — OpenCode hook

**Task:** Install the OpenCode equivalent.

**Acceptance:**

- Install/uninstall is idempotent.
- The hook runs on session idle or tool completion.
- The same fast path and budget apply.
- The output is a one-screen summary plus JSON.

**Evidence:** integration test; manual run in OpenCode.

### 5.4 — Fast-path budget

**Task:** Measure the hook on the reference app.

**Acceptance:**

- Delta (warm) ≤ 20 s.
- Fast checks ≤ 40 s.
- Total ≤ 60 s on a 1,000-file app.
- Over budget → delta-only with `checks: skipped-budget`.
- The measurement is recorded with hardware and versions.

**Read:** [`../docs/07-agent-and-distribution.md`](../docs/07-agent-and-distribution.md) §2.2.

**Evidence:** benchmark report.

### 5.5 — Hook output and brief

**Task:** Implement the human summary and the `CleanupBrief` JSON.

**Acceptance:**

- The summary shows the top three items and one next command.
- The brief is capped at 20 items and 32 KB.
- The brief contains no source text beyond names and paths.
- The brief is schema-valid.
- A snapshot test covers a multi-file case.

**Read:** [`../docs/07-agent-and-distribution.md`](../docs/07-agent-and-distribution.md) §2.4–§2.5.

**Evidence:** snapshot tests.

### 5.6 — Hook safety

**Task:** Prove the hook is local-only and non-mutating.

**Acceptance:**

- No network call (test with a blocked socket).
- No source upload.
- No write outside `.shrike/`.
- A 60-second hard timeout.
- Missing engine → exit 0 with a message.
- The hook is idempotent.

**Read:** [`../docs/09-security-and-trust.md`](../docs/09-security-and-trust.md) §3–§4.

**Evidence:** security test.

### 5.7 — GitHub Action

**Task:** Ship an Action with delta and prune modes.

**Acceptance:**

- Delta mode is read-only and uploads JSON.
- Prune mode opens a draft PR on a test repo.
- Fork PRs cannot run prune with write permissions.
- Inputs and defaults match
  [`../docs/07-agent-and-distribution.md`](../docs/07-agent-and-distribution.md) §4.1.
- The Action uses minimum permissions.
- Actions are pinned by commit SHA.

**Evidence:** CI log; test repository PR.

### 5.8 — PR template and receipt comment

**Task:** Render the PR body and receipt comment.

**Acceptance:**

- One commit per batch.
- The body includes the batch table, rejected list, and orphan-delta count.
- The receipt comment links the JSON artifact and shows the content hash.
- Logs are truncated and redacted.
- A snapshot test covers a partial rejection.

**Read:** [`../docs/05-verified-deletion.md`](../docs/05-verified-deletion.md) §9.

**Evidence:** snapshot tests.

### 5.9 — MCP wrapper

**Task:** Ship four read-only tools over stdio.

**Acceptance:**

- `shrike_delta`, `shrike_plan`, `shrike_proof`, `shrike_receipt`.
- No mutation.
- No project checks.
- Paths outside the working directory are refused.
- Output is capped at 256 KB.
- The server is opt-in.

**Read:** [`../docs/07-agent-and-distribution.md`](../docs/07-agent-and-distribution.md) §5.

**Evidence:** protocol test.

### 5.10 — Package naming and publish dry run

**Task:** Confirm the npm name and verify the tarball.

**Acceptance:**

- The name is available, or a scope is chosen.
- The tarball contains the launcher, license, README, and platform optional
  dependencies.
- It does not contain source, tests, or fixtures.
- `npm publish --dry-run` succeeds.
- The binary checksum is verified by the launcher.

**Read:** [`../docs/01-architecture.md`](../docs/01-architecture.md) §13.

**Evidence:** registry check; dry-run output.

### 5.11 — Landing test

**Task:** Test two positionings with qualified sign-ups.

**Acceptance:**

- A: "Delete what your agent left behind. Prove nothing broke."
- B: "The cleanup pass for AI-generated code."
- Measure CLI installs or repository connections, not page views.
- Run for at least two weeks or 1,000 qualified visitors.
- Record the result in the tracker.

**Read:** [`../docs/11-market-research.md`](../docs/11-market-research.md) §7.

**Evidence:** experiment report.

### 5.12 — Teardown content

**Task:** Publish a 10-repo teardown with before/after receipts.

**Acceptance:**

- Ten public Next.js repositories, pinned by commit.
- For each: the delta, the removed lines, the verified checks, and the receipt.
- No repository is misrepresented; license and attribution respected.
- The post links the raw receipts.

**Evidence:** published post; URL in the tracker.

### 5.13 — Design-partner kit

**Task:** Package a one-command trial.

**Acceptance:**

- Read-only first: `shrike delta` and `shrike plan`, no writes.
- A short guide explains the trust boundary and the check configuration.
- A triage template captures false positives and blocked findings.
- Removal is one command.

**Read:** [`../prompts/07-design-partner-triage-agent.md`](../prompts/07-design-partner-triage-agent.md).

**Evidence:** kit doc.

### 5.14 — Design partners

**Task:** Recruit and run three teams.

**Acceptance:**

- Three teams with a Next.js app and CI.
- Each runs read-only for one week.
- At least one merged Shrike PR per team by day 30.
- Every false positive becomes a corpus entry.
- Results are recorded with permission.

**Evidence:** tracker entry per team (redacted).

### 5.15 — Launch

**Task:** Publish the launch post, demo, and receipt screenshots.

**Acceptance:**

- The post states the product sentence and the proof.
- The demo shows a real PR, not a mock.
- Receipt screenshots are from real runs.
- Every performance claim links a raw report.
- The install path is tested on a clean machine.

**Evidence:** URL and checklist.

### 5.16 — Docs update

**Task:** Update `docs/07` and `docs/11` to match the shipped distribution.

**Acceptance:**

- Hook events, Action inputs, MCP tools, and package names are accurate.
- Open questions for PLAN-5 are resolved or reassigned.
- The tracker links the updated docs.

**Evidence:** review.

---

## 4. Exit gate

- [ ] Hooks install, run, and uninstall on Claude Code, Cursor, and OpenCode.
- [ ] The hook completes within 60 s or degrades to delta-only.
- [ ] The hook never applies a deletion and makes no network call.
- [ ] The Action opens a draft PR on a test repository.
- [ ] The Action refuses prune on a fork.
- [ ] The MCP server exposes exactly four read-only tools.
- [ ] The npm dry run is clean and the launcher verifies the binary checksum.
- [ ] The landing test has a result and a recommendation.
- [ ] Ten teardowns are published with receipts.
- [ ] Three design partners are active; at least one merged PR per team.
- [ ] The launch post links reproducible evidence.

---

## 5. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Hook is noisy and gets uninstalled | One screen, top three, one next command |
| Hook slows the agent | 60 s hard budget; degrade to delta-only |
| Action token scope too broad | Minimum permissions; fork refusal |
| MCP becomes a second product | Four read-only tools; no checks |
| Package name unavailable | Check early; scope fallback |
| Landing test attracts service buyers | Measure qualified installs, not views |
| Design partners churn | Read-only first; triage false positives fast |
| Launch claims outrun evidence | Every claim links a raw report |

---

## 6. Definition of done

A developer installs Shrike, runs it after an agent task, sees the code the task
orphaned, opens a verified cleanup PR from CI, and can show a receipt that proves
the app still builds — and three real teams have done it.
