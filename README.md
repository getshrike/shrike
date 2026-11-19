# Shrike — Proof-Carrying Cleanup

**Shrike deletes the code that agents and PRs leave behind — and proves the app
still builds.**

Shrike is a greenfield, Rust-powered cleanup tool for TypeScript and JavaScript
repositories, with first-class Next.js and React support. It is not a linter, a
score, a bundle optimizer, or another read-only dead-code report. It produces a
reviewable cleanup PR: independent deletion batches, the project's own checks run
against a baseline, failures bisected and reverted, and a portable receipt that
records exactly what was removed and what was verified.

This repository contains the **design and working implementation** of Shrike
1.0. The Rust engine, TypeScript host, CLI, verifier, receipts, hooks, GitHub
Action, and MCP server are implemented. Phase 4 (Next.js build-graph evidence)
has a working join; the full JSONL/route/byte/feedback contract is still in
progress. External launch items (design partners, npm publish, signed releases)
are tracked as blocked in [`TRACKER.md`](TRACKER.md).

---

## The problem in one paragraph

AI-assisted development changed the shape of technical debt. Agents add helpers,
fallbacks, variants, and second data-fetching paths; they rarely subtract. The old
caller survives the rewrite, the old helper survives the replacement, and the old
file survives the removed route. Tests and lint still pass because the residue is
unreachable, not incorrect. The codebase grows faster than it is understood, and
the next agent writes a fourth version of the same function. Detection tools tell
you what looks unused. None of them take responsibility for removing it safely.

Shrike does.

---

## What makes Shrike different

Most tools stop at detection. Shrike owns the risky part: **the deletion and the
proof**.

| Pillar | What Shrike does | Why it is different |
|---|---|---|
| **Whole-graph PR orphan delta** | Compares the merge-base graph to the PR head and reports code that became dead anywhere in the repo, including files the PR never touched | Changed-file audits cannot see this; current-state scanners cannot attribute it |
| **Verified deletion batches** | Applies each closure-complete batch in isolation, runs the project's checks against a baseline, bisects failures, and reverts only the culprit | Advisory tools stop at "safe to delete"; Shrike produces a reviewable PR |
| **Portable receipt** | Emits a content-addressed JSON proof: graph hashes, witnesses, patches, checks, baseline status, build evidence, outcome | A receipt is auditable and shareable; a score is not |
| **Build-graph evidence** | Joins build evidence to confirm presence and, as the phase-4 contract lands, report route and byte impact and catch dynamic edges the static graph missed | Optional confirmation, never a hard dependency |
| **Rust speed and determinism** | Parses and indexes 10,000 files in seconds with bounded memory; identical input produces byte-identical output | Fast enough for a post-agent hook; deterministic enough for CI |

The wedge is not "we find dead code." It is:

> **Shrike finds what this PR orphaned, deletes it in verified batches, and gives
> you a receipt that proves the app still builds.**

---

## Status

| Item | State |
|---|---|
| Product thesis | Complete — [`docs/00-thesis-and-positioning.md`](docs/00-thesis-and-positioning.md) |
| Architecture | Complete — [`docs/01-architecture.md`](docs/01-architecture.md) |
| Algorithms | Complete — [`docs/03-algorithms.md`](docs/03-algorithms.md) |
| Execution plans | Complete — [`plans/`](plans/) |
| Agent prompts | Complete — [`prompts/`](prompts/) |
| Research | Complete — [`research/`](research/) |
| Schemas | Complete and validated — [`schemas/`](schemas/) |
| Rust engine | Implemented — `cargo test --workspace` |
| TypeScript host | Implemented — `pnpm -r typecheck`, `pnpm test:node` |
| Fixtures and corpus | 14 delta, 6 verification, 250 must-not-report entries |
| Verified deletion | Implemented — `node scripts/e2e-prune.mjs` |
| Build-graph evidence | Partial — join works; JSONL/routes/bytes/feedback pending |
| External launch | Blocked on partners and credentials — see [`TRACKER.md`](TRACKER.md) |

The full status, decisions, metrics, and evidence log live in
[`TRACKER.md`](TRACKER.md).

---

## Repository map

```text
.
├── MASTER_PROMPT.md          # canonical product + engineering brief
├── README.md                 # this file
├── TRACKER.md                # status, tasks, decisions, metrics, logs
├── docs/                     # design documents
│   ├── 00-thesis-and-positioning.md
│   ├── 01-architecture.md
│   ├── 02-folder-structure.md
│   ├── 03-algorithms.md
│   ├── 04-whole-graph-delta.md
│   ├── 05-verified-deletion.md
│   ├── 06-build-graph-and-nextjs.md
│   ├── 07-agent-and-distribution.md
│   ├── 08-benchmarks-and-confidence.md
│   ├── 09-security-and-trust.md
│   ├── 10-non-goals-and-risks.md
│   ├── 11-market-research.md
│   └── 12-decision-log.md
├── plans/                    # execution plans, in dependency order
│   ├── PLAN-0-foundations.md
│   ├── PLAN-1-rust-engine.md
│   ├── PLAN-2-graph-and-delta.md
│   ├── PLAN-3-verification-and-receipts.md
│   ├── PLAN-4-build-graph-and-nextjs.md
│   ├── PLAN-5-distribution-and-launch.md
│   └── PLAN-6-hardening-and-scale.md
├── prompts/                  # role prompts for implementation and review agents
├── research/                 # dated research, competitor matrix, sources
├── schemas/                  # JSON Schemas for protocol, snapshots, plans, receipts
├── fixtures/                 # delta, verification, build, receipt, and corpus fixtures
├── benchmarks/               # deterministic 1k/10k benchmark generator and harness
├── crates/                   # Rust workspace: core, protocol, syntax, resolve, graph, delta, plan, evidence, receipt, engine
├── packages/                 # TypeScript host: cli, verifier, receipts, agent, action, testkit
└── scripts/                  # repository automation and contract gates
```

The `crates/`, `packages/`, `fixtures/`, `benchmarks/`, and `scripts/` directories
contain working code and are exercised by `.github/workflows/ci.yml`.

---

## Quick start

```bash
# Build the Rust engine
cargo build --release -p shrike-engine

# Run the local gates
pnpm install
pnpm -r typecheck
pnpm test:node
cargo test --workspace
node scripts/check.mjs

# Use the CLI from a repository
node /path/to/shrike/packages/cli/src/main.ts scan --json
node /path/to/shrike/packages/cli/src/main.ts delta --base origin/main --json
node /path/to/shrike/packages/cli/src/main.ts prune --base origin/main --json
```

`prune` defaults to a branch worktree under `.shrike/worktrees/`, writes a
receipt per batch under `.shrike/receipts/`, and never commits to `main`.
`--in-place` is explicit opt-in. Exit codes are `0` clean, `1` findings,
`2` usage/config, `3` verification failed, `4` could not run, and `5` revert
failed.

---

## The pipeline

```text
project + config
  → parse (oxc, Rust)
  → semantic summaries
  → module resolution
  → graph + framework entries
  → reachability with witness paths
  → base snapshot + head snapshot
  → orphan delta (what this PR killed)
  → SCC condensation + cascade closure
  → independent deletion batches + blast-radius check
  → apply batch in a worktree
  → baseline checks → after checks → compare
  → pass: keep + receipt
  → fail: bisect, revert culprit, retry remainder
  → draft PR + portable receipt
```

The full algorithm is in [`docs/03-algorithms.md`](docs/03-algorithms.md). The
delta is in [`docs/04-whole-graph-delta.md`](docs/04-whole-graph-delta.md). The
verifier is in [`docs/05-verified-deletion.md`](docs/05-verified-deletion.md).

---

## How to read this package

### If you are a human

1. Read [`docs/00-thesis-and-positioning.md`](docs/00-thesis-and-positioning.md)
   for the problem, buyer, and category.
2. Read [`docs/01-architecture.md`](docs/01-architecture.md) and
   [`docs/03-algorithms.md`](docs/03-algorithms.md) for how it works.
3. Read [`docs/10-non-goals-and-risks.md`](docs/10-non-goals-and-risks.md) for
   what it deliberately will not do.
4. Start executing at [`plans/PLAN-0-foundations.md`](plans/PLAN-0-foundations.md).

### If you are an implementation agent

1. Load [`prompts/00-orchestrator.md`](prompts/00-orchestrator.md).
2. Read [`MASTER_PROMPT.md`](MASTER_PROMPT.md) and the doc named by the task.
3. Work one plan at a time. Do not skip the exit gate.
4. Update [`TRACKER.md`](TRACKER.md) with evidence, not claims.

### If you are reviewing the design

Start with [`research/01-competitive-landscape.md`](research/01-competitive-landscape.md)
and [`docs/12-decision-log.md`](docs/12-decision-log.md). The decision log records
what was rejected and why.

---

## Non-goals

Shrike will not become:

- another bundle optimizer (Vercel owns that);
- a 100+ plugin catalogue (Knip and Fallow own that);
- a multi-language analyzer (aislop and fossil own that);
- a slop score or cleanliness badge (the wrong metric);
- a type checker (the project's typecheck is a verification input);
- a generic MCP dead-code scanner (crowded and undifferentiated);
- an auto-committer to `main` (PR-first, always);
- a hosted dashboard before the deletion loop is proven.

Full reasoning: [`docs/10-non-goals-and-risks.md`](docs/10-non-goals-and-risks.md).

---

## Success in one table

| Metric | Target |
|---|---|
| Confirmed false positives on the must-not-report corpus | 0 |
| Proposed batches that verify without manual edits | ≥ 80% |
| Real PRs where the orphan delta finds ≥ 1 newly dead file | ≥ 30% |
| 1,000-file cold scan | ≤ 1.5 s |
| 1,000-file warm scan | ≤ 400 ms |
| 10,000-file cold scan | ≤ 6 s |
| 10,000-file peak RSS | ≤ 600 MB |
| Fast-path receipt (delta + typecheck + lint) | ≤ 60 s |
| Full receipt (typecheck + lint + test + build) | ≤ 10 min |
| Design partners who merge a Shrike PR in month one | ≥ 3 |

These are targets, not results. Actuals are recorded in
[`TRACKER.md`](TRACKER.md) §5.

---

## Working name

`shrike` is the working name and the CLI binary. The npm package name and any
scope are confirmed in [`PLAN-5`](plans/PLAN-5-distribution-and-launch.md). The
Rust crates use the `shrike-` prefix.

## License

Unlicensed draft. The license is decided before the first public release; see
[`PLAN-5`](plans/PLAN-5-distribution-and-launch.md).
