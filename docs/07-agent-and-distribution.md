# 07 — Agent and Distribution

> **Parent:** [MASTER_PROMPT.md](../MASTER_PROMPT.md) · **Tracker:** [TRACKER.md](../TRACKER.md)
> **Status:** canonical · Authored 2026-11-20
> **Implementation status (2026-11-23):** Claude Code, Cursor, and OpenCode hooks, the GitHub Action, the PR/receipt renderers, the four-tool read-only MCP server, the read-only [design-partner kit](../design-partner/README.md), and the npm release path are implemented. The hook runs fast-tier checks against a reused baseline inside its budget; the Action supports `fail-on: newly-dead`, resolves a project-local `getshrike` install, replaces/removes its delta artifact, and runs from a committed bundle. `getshrike@1.0.0` and the five native platform packages are published with SHA-256 sidecars, GitHub attestations, keyless Sigstore signing, and npm provenance. External delta and prune runs passed on `getshrike/shrike-action-test`, including a draft cleanup PR with a verified receipt. Landing tests, design-partner trials, and the independent audit remain blocked on participants.

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
The delta runs in a child process with `--allow-dirty`. Checks are the
configured `tier: "fast"` checks, or typecheck/lint discovered from
`package.json` when no checks are configured. A persisted baseline is reused
only when its head commit matches and every fast check matches a stored entry
by name and command, so a standard-tier baseline captured by `prune` can still
explain a pre-existing failure. The hook never captures a baseline because that
work belongs to `prune`. Check failures are advisory and never change the
hook's exit code.

The brief's `checks` field is one of `pass`, `fail`, `timeout`, `error`,
`skipped` (no fast checks configured or discoverable), or `skipped-budget`
(the remaining budget was too small to start or finish a check).

### 2.2 Budget

| Step | Budget |
|---|---|
| Delta (warm) | ≤ 20 s |
| Fast checks | ≤ 40 s |
| Total | ≤ 60 s |

If the budget is exceeded, the hook degrades to delta-only, marks the brief
`checks: skipped-budget`, and exits 0. It never blocks the agent for minutes.
The delta child process is capped at 55 s, and the checks share one deadline
derived from the total budget. Tests may shorten the budgets with
`SHRIKE_HOOK_TOTAL_BUDGET_MS` and `SHRIKE_HOOK_CHECKS_BUDGET_MS`.

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
- The hook writes only under `.shrike/`; the project's own fast checks may write
  their normal build caches, but the hook never edits source or applies a
  deletion.
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
| `mode` | `delta` | `delta` (read-only) or `prune` (verified cleanup PR) |
| `base` | `origin/${{ github.base_ref }}` | Base ref |
| `max-items` | `0` | Fail delta mode when the count exceeds this value; `0` disables |
| `fail-on` | `never` | `newly-dead` fails delta mode on any finding; `never` fails only above `max-items` |

Outputs: `count`, `delta-path` (delta mode); `outcome`, `receipts` (prune mode).
The Action is a thin wrapper over the CLI; it does not re-implement analysis.
`mode: prune` refuses a fork pull request and returns exit 2. When the runner
has no git identity, prune commits as `github-actions[bot]`; a configured
identity is left unchanged.

### 4.2 Permissions

- `contents: read` for delta mode;
- `contents: write`, `pull-requests: write` for prune mode;
- the repository setting **Allow GitHub Actions to create and approve pull
  requests** when prune uses the default `GITHUB_TOKEN`; without it,
  `gh pr create` is rejected and prune exits 4;
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
| `shrike hook run` | One bounded delta for an agent lifecycle hook | cache |
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
npm install -D getshrike
npx getshrike scan
npx shrike scan        # alias for the same launcher

# standalone binary from a GitHub release
curl -fsSL https://getshrike.dev/install.sh | sh
```

- The unscoped npm package is `getshrike`; the command remains `shrike`, and
  the package also exposes a `getshrike` bin alias.
- The package ships a small launcher and five optional platform packages
  containing the native engine: `getshrike-linux-x64`,
  `getshrike-linux-arm64`, `getshrike-darwin-x64`,
  `getshrike-darwin-arm64`, and `getshrike-windows-x64`.
- The launcher resolves `SHRIKE_ENGINE`, a repository `target/{release,debug}`
  build, the installed platform package, then `PATH`. A packaged binary must
  match its `.sha256` sidecar before it runs; a missing or mismatched sidecar
  fails closed with exit 4. Repository builds and `SHRIKE_ENGINE` overrides
  verify a sidecar only when one is present.
- A missing platform package fails closed with a clear message that names the
  expected package and the install command.
- No postinstall script downloads an unverified binary.
- `node scripts/check-package.mjs` packs the real tarballs with `pnpm pack` and
  checks the manifest, file list, bundle boundary, platform metadata, and
  `pnpm publish --dry-run`. `node scripts/test-install.mjs` installs the packed
  tarballs into an empty project, runs both bins, runs a real scan, and asserts
  the checksum-mismatch and missing-platform failure modes.
- `scripts/stage-platform.mjs` copies a native binary into its platform package
  with a SHA-256 sidecar, README, and license. The release workflow runs it for
  each target and publishes from the exact attested tarballs.

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

- [x] `shrike hook install --claude` is idempotent and does not duplicate the
      hook block.
- [x] The hook completes within 60 s on a 1,000-file app or degrades to
      delta-only.
- [x] The hook never applies a deletion.
- [x] The hook writes only under `.shrike/` and makes no network call.
- [x] The Action opens a draft PR on a test repository (`getshrike/shrike-action-test`
      run 45085138639, PR #5, verified receipt `rcpt_8e5c5060b59af8480afcab34`).
- [x] The Action refuses `prune` on a fork.
- [x] The Action resolves a project-local `getshrike` install, honors
      `SHRIKE_CLI`, and falls back to `shrike` on `PATH`.
- [x] The MCP server exposes exactly four read-only tools.
- [x] The MCP server refuses paths outside the working directory.
- [x] The design-partner kit runs read-only delta and plan, explains the trust
      boundary, and removes with one command.
- [x] The agent brief is capped and strips instruction-like comments.
- [x] The CLI exits 4, not 0, when the engine is missing.
- [x] `--json` output validates against the schemas.
- [x] The packed `getshrike` and platform tarballs install into an empty project
      and both bins run a real scan.
- [x] The launcher fails closed on a checksum mismatch or a missing platform
      package.
- [ ] The release workflow completes on a `v1.0.0` tag and publishes the five
      platform packages plus `getshrike` with provenance (release gate: npm
      credentials and environment approval).
