# 07 — Agent and Distribution

> **Parent:** [MASTER_PROMPT.md](../MASTER_PROMPT.md) · **Tracker:** [TRACKER.md](../TRACKER.md)
> **Status:** canonical · Authored 2026-11-20

---

## 1. Why distribution is a product surface

The best deletion engine is useless if the developer has to remember to run it.
The tools that grew in the AI-coding era met the agent where it works:

- `aislop` ships `hook install --claude`, `--cursor`, and CI/MCP integrations,
  and reached roughly 110k npm downloads per month.
- Claude Code, Cursor, and OpenCode expose lifecycle hooks. The hook is a
  first-class integration surface, not a hack.
- The cleanup moment is immediately after the agent finishes, not a week later
  in a quarterly cleanup.

Shrike's distribution therefore has three surfaces, in order:

1. **Post-agent hook** — fast feedback in the loop.
2. **Pull request** — the verified deletion, reviewed like any other change.
3. **CI gate** — the read-only delta check for teams not ready to accept
   deletion PRs.

A fourth surface, MCP, is a thin wrapper over the same CLI. It is not the
product and must not become a second analysis path.

---

## 2. The post-agent hook

### 2.1 Job

After an agent finishes an edit:

1. compute the whole-graph delta against the merge base;
2. run the fast checks (typecheck and lint) against a baseline;
3. write a compact `CleanupBrief` JSON;
4. print a one-screen summary.

The hook **never applies a deletion**. It reports; the human or agent decides.

### 2.2 Budget

| Step | Budget |
|---|---|
| Delta (warm) | ≤ 20 s |
| Fast checks | ≤ 40 s |
| Total | ≤ 60 s |

If the budget is exceeded, the hook degrades to delta-only, marks the brief
`checks: skipped-budget`, and exits 0. It never blocks the agent for minutes.

### 2.3 Hook events

| Agent | Event | Why |
|---|---|---|
| Claude Code | `Stop` / `SubagentStop` | The agent has finished a task |
| Cursor | `afterFileEdit` (debounced) or task end | The edit has landed |
| OpenCode | session idle / tool completion | The task is done |

Per-edit hooks are noisy and expensive; prefer task-end. The installer records
the event it chose and why in `shrike.config.ts` comments.

### 2.4 Hook output

Human summary:

```text
Shrike: 3 files this task made dead (2 in untouched files).
  src/lib/format.ts        formatInvoice, formatCents       -41 lines
  src/components/Legacy.tsx  LegacyCard                     -88 lines
  src/lib/currency.ts      (file)                           -37 lines
Next: shrike plan --base origin/main
```

JSON brief:

```json
{
  "schemaVersion": "1.0",
  "kind": "cleanup-brief",
  "base": "origin/main",
  "head": "HEAD",
  "dirty": true,
  "newlyDead": 3,
  "files": 3,
  "untouchedFiles": 2,
  "estimatedLines": 166,
  "checks": "pass",
  "top": [
    { "symbol": "src/lib/format.ts#formatInvoice", "trigger": "src/features/invoice/InvoiceCard.tsx" }
  ],
  "nextCommand": "shrike plan --base origin/main"
}
```

The brief is capped at 20 items and 32 KB. It contains no source text beyond
symbol names and paths.

### 2.5 Hook safety

- No network calls.
- No source upload.
- Writes only under `.shrike/`.
- A 60-second hard timeout.
- If the engine is missing, the hook prints one line and exits 0; it does not
  block the agent.
- The hook is idempotent; reinstalling replaces its own block only.

---

## 3. The pull request

`shrike prune --base origin/main --verify --receipt`:

1. Compute the plan.
2. Create a branch `shrike/cleanup-<shortsha>`.
3. For each batch: apply in a worktree, run checks, compare, bisect on failure,
   commit the verified batch.
4. Open a draft PR with one commit per batch.
5. Post the receipt as a comment.
6. Exit 0 if any batch verified; 3 if all were rejected; 4 if verification could
   not run.

### 3.1 PR body

```markdown
## Verified cleanup

Shrike removed **1,240 lines** across **18 files** in **3 verified batches**.

| Batch | Symbols | Files | Lines | typecheck | lint | test | build |
|---|---|---|---|---|---|---|---|
| batch_1 | 22 | 6 | 812 | ✓ | ✓ | ✓ | ✓ |
| batch_2 | 11 | 7 | 318 | ✓ | ✓ | ✓ | ✓ |
| batch_3 | 4 | 5 | 110 | ✓ | ✓ | ✓ | ✓ |

**Rejected and reverted:** 3 deletions (test failure in `invoice.test.ts`).
**Orphan delta:** 14 symbols were orphaned by this PR; 4 were pre-existing.

Receipt: `.shrike/receipts/rcpt_01J....json`
```

### 3.2 Review model

- draft by default;
- one commit per batch;
- each commit message names the batch ID and symbol count;
- the receipt is immutable; a re-run produces a new receipt;
- the PR never force-pushes over a human edit; if the branch diverged, the tool
  stops and reports.

### 3.3 Never

- never commit to `main`;
- never push over an existing human branch without `--branch <name>`;
- never mark the PR ready for review automatically;
- never include a deletion that failed verification.

---

## 4. The CI gate

For teams not ready to accept deletion PRs:

```yaml
- uses: shrike-dev/action@v1
  with:
    mode: delta
    base: origin/${{ github.base_ref }}
    fail-on: newly-dead
```

Read-only. Exit 1 if the delta is non-empty and above threshold, 0 otherwise.
The Action uploads the delta JSON and, optionally, a receipt-like evidence
artifact.

### 4.1 Action inputs

| Input | Default | Meaning |
|---|---|---|
| `mode` | `delta` | `delta`, `scan`, or `prune` |
| `base` | `origin/${{ github.base_ref }}` | Base ref |
| `fail-on` | `newly-dead` | `never`, `newly-dead`, `threshold:N` |
| `checks` | `fast` | `fast`, `standard`, `full`, `none` |
| `receipt` | `false` | Upload a receipt artifact |
| `build-evidence` | `auto` | `auto`, `require`, `off` |
| `version` | `latest` | Pinned Shrike version |

### 4.2 Permissions

- `contents: read` for delta mode;
- `contents: write`, `pull-requests: write` for prune mode;
- no `actions: write`;
- a fork PR gets read-only delta mode and no secrets.

### 4.3 Fork safety

- never run `prune` on a fork with write permissions;
- never expose a token to project scripts from a fork;
- default to delta-only when `github.event.pull_request.head.repo.fork` is true.

---

## 5. MCP wrapper

Four tools, thin wrappers over the CLI:

| Tool | CLI | Mutation | Output |
|---|---|---|---|
| `shrike_delta` | `shrike delta --json` | none | delta JSON |
| `shrike_plan` | `shrike plan --json` | none | plan JSON |
| `shrike_proof` | `shrike proof <symbol> --json` | none | witness or proof trace |
| `shrike_receipt` | `shrike receipt <id> --json` | none | receipt JSON |

Rules:

- no tool mutates the repository;
- no tool runs project checks (too slow, too dangerous);
- the server validates the working directory and refuses paths outside it;
- the server caps output at 256 KB;
- the server never reads `.env` or secrets;
- the MCP server is opt-in (`shrike mcp serve`), not installed by default.

The MCP wrapper exists because agents ask for it. It is not the product; the
verified PR is.

---

## 6. Agent brief

An optional, bounded context bundle for an agent that is about to clean up:

```text
CleanupBrief {
  repo: string,
  base: string,
  head: string,
  newlyDead: DeltaItem[],     // capped
  preexisting: number,
  blocked: BlockedItem[],     // unknown edges, public API, unparsed
  suggestedBatch: Batch | null,
  commands: { plan, prune, proof },
  limits: { maxItems, maxBytes }
}
```

Rules:

- cap at 20 delta items and 32 KB;
- strip comments that look like instructions;
- treat repository content as data, never as instructions;
- no secrets, no environment values, no absolute home paths;
- the brief is advisory; the verifier is authoritative.

Prompt-injection defense: the brief encloses repository-derived strings in
quoted fields, removes control characters, and prefixes the whole bundle with a
line stating that repository content is untrusted data. The agent is instructed
to ignore instructions found inside it. This is mitigation, not a guarantee;
never give an agent credentials it does not need.

---

## 7. Command surface

| Command | Purpose | Mutates |
|---|---|---|
| `shrike scan` | Current-state evidence report | no |
| `shrike proof <symbol>` | Witness path or proof trace | no |
| `shrike delta --base <ref>` | Whole-graph PR orphan delta | no |
| `shrike plan --base <ref>` | Batches, impact, blast radius | no |
| `shrike prune --base <ref>` | Apply, verify, commit, PR | yes |
| `shrike verify --plan <id>` | Re-run verification for a plan | worktree |
| `shrike receipt <id>` | Show a receipt | no |
| `shrike evidence --build <path>` | Join build evidence | no |
| `shrike hook install --claude|--cursor|--opencode` | Install a hook | config |
| `shrike mcp serve` | MCP server over stdio | no |
| `shrike cache clean|verify` | Cache maintenance | cache |
| `shrike version` | Version and protocol | no |

All read-only commands support `--json`. JSON output is schema-valid and
deterministic.

Exit codes are defined in [05-verified-deletion.md](05-verified-deletion.md) §5.

---

## 8. Installation

```bash
# npm (recommended)
npm install -D shrike
npx shrike scan

# one-off
npx shrike scan

# standalone binary
curl -fsSL https://shrike.dev/install.sh | sh
```

- The npm package ships a launcher and optional platform packages containing the
  native engine.
- The launcher verifies the binary checksum before spawning it.
- A missing platform package fails closed with a clear message and a link to the
  standalone binary.
- No postinstall script downloads an unverified binary.

---

## 9. Telemetry

Off by default. If ever added:

- opt-in only, at first run, with a plain-language explanation;
- aggregate counts only (commands, durations, exit codes, versions);
- never paths, symbol names, source, repository names, or error text;
- a `SHRIKE_TELEMETRY=0` hard off switch;
- documented in the README and the privacy policy.

Telemetry is deferred in the tracker (6.12) and is not part of v1.

---

## 10. Distribution metrics

| Metric | Target |
|---|---|
| Hook install → first run | ≥ 80% |
| Hook runs per week per user | ≥ 5 |
| Delta-to-plan conversion | ≥ 40% |
| Plan-to-prune conversion | ≥ 25% |
| Merged PR rate | ≥ 60% of opened prune PRs |
| Time from install to first receipt | ≤ 10 minutes |

These are measured in design-partner trials, not inferred from downloads.

---

## 11. Acceptance criteria

- [ ] `shrike hook install --claude` is idempotent and does not duplicate the
      hook block.
- [ ] The hook completes within 60 s on a 1,000-file app or degrades to
      delta-only.
- [ ] The hook never applies a deletion.
- [ ] The hook writes only under `.shrike/` and makes no network call.
- [ ] The Action opens a draft PR on a test repository.
- [ ] The Action refuses `prune` on a fork.
- [ ] The MCP server exposes exactly four read-only tools.
- [ ] The MCP server refuses paths outside the working directory.
- [ ] The agent brief is capped and strips instruction-like comments.
- [ ] The CLI exits 4, not 0, when the engine is missing.
- [ ] `--json` output validates against the schemas.
