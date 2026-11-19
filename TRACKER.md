# Shrike 1.0 — Tracker

> **Canonical design:** [`MASTER_PROMPT.md`](MASTER_PROMPT.md)
> **Status:** design complete · implementation not started
> **Last updated:** 2026-11-20

This tracker is authoritative for Shrike 1.0 status. A phase is not done because
the document is long or the code compiles; it is done when the exit gate is
demonstrated and the evidence is linked here.

---

## 1. Status legend

| Status | Meaning |
|---|---|
| `planned` | Scoped, not started |
| `in-progress` | Work has begun; evidence is partial |
| `blocked` | Cannot proceed; reason recorded |
| `done` | Exit gate passed; evidence recorded |
| `deferred` | Intentionally out of this version |

---

## 2. Current position

- **Product direction:** complete. Shrike 1.0 is a greenfield, Rust-powered
  verified-cleanup tool. The wedge is the whole-graph PR orphan delta plus
  verified deletion batches and a portable receipt.
- **Design package:** complete and validated in this repository. The root contains
  the master prompt, 13 design docs, 7 plans, 9 prompts, 8 research documents, and
  6 JSON Schemas. Link, task-ID, decision-ID, and schema-structure checks pass.
- **Implementation:** not started. Begin at
  [`plans/PLAN-0-foundations.md`](plans/PLAN-0-foundations.md).
- **Prior prototype:** not a dependency. The old tree was removed; a backup
  exists outside the repository only as a safety net. No code, schema, or fixture
  from it is reused.

---

## 3. Phase overview

| Phase | Plan | Status | Exit gate |
|---|---|---|---|
| 0 | [Foundations](plans/PLAN-0-foundations.md) | `planned` | Contract, schemas, fixtures, CI green |
| 1 | [Rust engine](plans/PLAN-1-rust-engine.md) | `planned` | Engine builds, fixtures pass, budgets met |
| 2 | [Graph and delta](plans/PLAN-2-graph-and-delta.md) | `planned` | Orphan delta correct; corpus clean |
| 3 | [Verification and receipts](plans/PLAN-3-verification-and-receipts.md) | `planned` | End-to-end verified deletion PR + receipt |
| 4 | [Build graph and Next.js](plans/PLAN-4-build-graph-and-nextjs.md) | `planned` | Build evidence joined; byte impact reported |
| 5 | [Distribution and launch](plans/PLAN-5-distribution-and-launch.md) | `planned` | Hook + Action + 3 design partners |
| 6 | [Hardening and scale](plans/PLAN-6-hardening-and-scale.md) | `planned` | 1.0 release: corpus, security, scale, docs |

---

## 4. Tasks

### Phase 0 — Foundations

| ID | Task | Status | Acceptance | Evidence |
|---|---|---|---|---|
| 0.1 | Confirm greenfield layout | `planned` | Root `crates/`, `packages/`, `schemas/`, `fixtures/`, `benchmarks/`, `scripts/` named; no legacy dependency | `docs/02-folder-structure.md` |
| 0.2 | Write engine/host protocol | `planned` | Versioned JSON; valid, malformed, timeout, crash, partial-result cases | `docs/01-architecture.md` §6; fixtures |
| 0.3 | Write `GraphSnapshot` schema | `planned` | Files, symbols, modules, entries, edges, certainty, hashes, version | `schemas/graph-snapshot.schema.json` |
| 0.4 | Write `Delta` schema | `planned` | Newly dead symbols/files; witness; trigger; cascade; batches | `schemas/delta.schema.json` |
| 0.5 | Write `DeletionPlan` schema | `planned` | Batches, impact, blast radius, patches | `schemas/deletion-plan.schema.json` |
| 0.6 | Write `Receipt` schema | `planned` | Batch, checks, baseline status, outcome, rejected, environment | `schemas/receipt.schema.json` |
| 0.7 | Write `Config` schema | `planned` | Include/ignore, entries, checks, policy, build evidence | `schemas/config.schema.json` |
| 0.8 | Scaffold Rust workspace | `planned` | `cargo test`, `cargo clippy`, `cargo fmt --check` green | CI log |
| 0.9 | Scaffold TypeScript packages | `planned` | `pnpm test`, `pnpm typecheck`, `pnpm lint` green | CI log |
| 0.10 | Build fixture harness | `planned` | Temp repos, fixture checkout, snapshot assertions | `packages/testkit/` |
| 0.11 | Build must-not-report corpus | `planned` | ≥ 100 entries from framework docs and real apps | `fixtures/must-not-report/` |
| 0.12 | Build delta fixtures | `planned` | Nine scenarios from `docs/04` §11 | `fixtures/delta/` |
| 0.13 | Build verification fixtures | `planned` | Apply, bisect, revert, flaky, known-failure | `fixtures/verification/` |
| 0.14 | Wire CI | `planned` | Rust, Node, schemas, corpus, fixtures, bench smoke | `.github/workflows/` |
| 0.15 | Record decisions and tracker | `planned` | D-001…D-018 recorded; phase tables match plans | `TRACKER.md` §6 |

### Phase 1 — Rust engine

| ID | Task | Status | Acceptance | Evidence |
|---|---|---|---|---|
| 1.1 | `shrike-core`: IDs, interners, spans, errors | `planned` | Deterministic IDs and stable serialization | unit tests |
| 1.2 | `shrike-protocol`: envelope and codec | `planned` | Line-delimited JSON; versioned; malformed fails closed | contract tests |
| 1.3 | `shrike-syntax`: parse + per-file summary | `planned` | Imports, exports, re-exports, JSX, directives, dynamic forms | fixture tests |
| 1.4 | `shrike-syntax`: semantic scopes/symbols/references | `planned` | `oxc_semantic` facts captured; no AST retention | fixture tests |
| 1.5 | `shrike-resolve`: resolver | `planned` | Relative, alias, workspace, package exports, barrel; unresolved recorded | graph fixtures |
| 1.6 | `shrike-graph`: snapshot + module graph | `planned` | Files, modules, symbols, edges, entries, hashes | unit tests |
| 1.7 | Entry adapters: Next.js App Router | `planned` | Pages, layouts, routes, metadata, server actions, client boundaries | `next-app` fixture |
| 1.8 | Entry adapters: Pages Router + React | `planned` | Pages, API routes, `_app`, `_document`, data functions, React roots | `next-pages` fixture |
| 1.9 | Reachability + witnesses | `planned` | Every live symbol has a shortest witness; every dead symbol has a proof trace | invariant tests |
| 1.10 | Dynamic-edge abstraction | `planned` | Literal, template, glob, namespace, string-keyed, unknown | dynamic-edge fixtures |
| 1.11 | SCC condensation | `planned` | Cycles treated as units; no partial-cycle deletion | graph fixtures |
| 1.12 | Snapshot cache + warm path | `planned` | Content-addressed; warm scan skips unchanged files; atomic writes | cache tests |
| 1.13 | `shrike-engine` binary + commands | `planned` | `snapshot`, `delta`, `plan`, `proof`, `evidence`, `version` | contract tests |
| 1.14 | Determinism harness | `planned` | Two runs and different thread counts produce byte-identical snapshots | CI log |
| 1.15 | Performance harness | `planned` | 1k/10k cold, warm, peak RSS recorded against budgets | `benchmarks/` |
| 1.16 | Fail-closed errors | `planned` | Missing binary, malformed response, timeout, crash all exit 4 | integration test |
| 1.17 | CLI integration: `scan`, `proof` | `planned` | Output matches fixture oracle; exit codes correct | integration test |

### Phase 2 — Graph and delta

| ID | Task | Status | Acceptance | Evidence |
|---|---|---|---|---|
| 2.1 | Base snapshot acquisition | `planned` | Merge-base worktree or archive; read-only; cached | fixture test |
| 2.2 | Head snapshot | `planned` | Commit or dirty tree; `dirty` flag | fixture test |
| 2.3 | Comparability gate | `planned` | Refuses incomparable inputs with a named reason | unit test |
| 2.4 | Live-set diff | `planned` | `live(base) − live(head)`; no pre-existing debt | fixture test |
| 2.5 | Witness lookup | `planned` | Lost witness path present for every delta item | fixture test |
| 2.6 | Trigger attribution | `planned` | Names the changed file/symbol that removed the edge | fixture test |
| 2.7 | Cascade closure | `planned` | Exclusive ownership; stop conditions; unknown blocks | fixture test |
| 2.8 | Batch conflict graph | `planned` | No shared files/SCC/order within a batch | unit test |
| 2.9 | Blast-radius guard | `planned` | Batch over limit is split or refused with a reason | unit test |
| 2.10 | Impact estimation | `planned` | Files, lines, symbols, routes; no double counting | fixture test |
| 2.11 | `shrike delta` | `planned` | JSON + human output; schema-valid | integration test |
| 2.12 | `shrike plan` | `planned` | Batches with impact and blast radius | integration test |
| 2.13 | Delta fixtures + corpus | `planned` | Nine scenarios; zero corpus findings | fixture suite |
| 2.14 | Monorepo handling | `planned` | Workspace boundaries respected | monorepo fixture |
| 2.15 | Determinism | `planned` | Byte-identical output across runs and thread counts | CI log |
| 2.16 | Real-app delta | `planned` | A synthetic feature removal produces untouched-file orphans | report |
| 2.17 | Docs update | `planned` | `docs/04` matches behavior | review |

### Phase 3 — Verification and receipts

| ID | Task | Status | Acceptance | Evidence |
|---|---|---|---|---|
| 3.1 | Verifier package scaffold | `planned` | Public API, errors, config | unit tests |
| 3.2 | Baseline capture | `planned` | Commit, checks, signatures, environment recorded | fixture test |
| 3.3 | Check discovery | `planned` | `package.json` scripts + config; explicit commands win | unit test |
| 3.4 | Worktree mode | `planned` | Create, apply, verify, remove; clean tree required | integration test |
| 3.5 | In-place mode | `planned` | Snapshot, apply, verify, restore; dirty-tree refusal | integration test |
| 3.6 | Compare/classification | `planned` | pass, improvement, known-failure, new-failure, fail, flaky, timeout | unit test |
| 3.7 | Batch apply | `planned` | Independent batches verified separately | integration test |
| 3.8 | Bisection | `planned` | Minimal failing set isolated and reverted | failure drill |
| 3.9 | Revert/recovery | `planned` | Clean restore or exit 5 with recovery path | failure drill |
| 3.10 | Receipt writer | `planned` | Schema-valid; content-addressed; reproducible | schema test |
| 3.11 | Receipt validation | `planned` | Every fixture receipt validates | CI log |
| 3.12 | PR renderer | `planned` | Markdown summary, per-batch table, rejected list | snapshot test |
| 3.13 | Per-batch commits + draft PR | `planned` | One commit per verified batch; draft by default | integration test |
| 3.14 | Exit codes | `planned` | 0/1/2/3/4/5 covered | CLI test |
| 3.15 | Flaky handling | `planned` | Rerun, classify, record; no false regression | integration test |
| 3.16 | Failure drill | `planned` | Broken deletion reverted; tree clean | CI log |
| 3.17 | Real-app end-to-end | `planned` | 3 batches verified with typecheck, lint, test, build | receipt |
| 3.18 | Docs update | `planned` | `docs/05` matches behavior | review |

### Phase 4 — Build graph and Next.js

| ID | Task | Status | Acceptance | Evidence |
|---|---|---|---|---|
| 4.1 | Snapshot discovery | `planned` | Finds the snapshot; runs export only with `--build` | integration test |
| 4.2 | Version guard | `planned` | 16.4+ full; 16.1–16.3 partial; older unsupported | unit test |
| 4.3 | JSONL parser | `planned` | Streaming; tolerates additive fields | fixture test |
| 4.4 | Module index | `planned` | Path and ID lookup; normalized | unit test |
| 4.5 | Module-to-file join | `planned` | ≥ 90% coverage on a real app; unresolved recorded | integration test |
| 4.6 | Presence/absence | `planned` | Client/server distinction; suppress on presence | fixture test |
| 4.7 | Route attribution | `planned` | Per-route membership; shared chunks attributed once | integration test |
| 4.8 | Byte impact | `planned` | Per-route and total; no double counting | integration test |
| 4.9 | Build-edge feedback | `planned` | Adds certain edges; suppresses false positives | fixture test |
| 4.10 | Server actions | `planned` | Client-reference manifest considered; conservative fallback | fixture test |
| 4.11 | Client boundaries | `planned` | `"use client"` entry semantics; leak diagnostic | fixture test |
| 4.12 | Proxy/middleware/metadata | `planned` | Entry and matcher semantics; never dead | fixture test |
| 4.13 | Fallback matrix | `planned` | Missing/stale/parse-error/unsupported → static-only with status | integration test |
| 4.14 | Receipt integration | `planned` | Build evidence status and hash recorded | schema test |
| 4.15 | Corpus with/without evidence | `planned` | Green in both modes | corpus report |
| 4.16 | Docs update | `planned` | `docs/06` matches behavior | review |

### Phase 5 — Distribution and launch

| ID | Task | Status | Acceptance | Evidence |
|---|---|---|---|---|
| 5.1 | Claude Code hook | `planned` | Install, run, uninstall; idempotent | integration test |
| 5.2 | Cursor hook | `planned` | Same fast path | integration test |
| 5.3 | OpenCode hook | `planned` | Same fast path | integration test |
| 5.4 | Fast-path budget | `planned` | ≤ 60 s or delta-only | benchmark |
| 5.5 | Hook output + brief | `planned` | One-screen summary + JSON `CleanupBrief` | snapshot test |
| 5.6 | Hook safety | `planned` | No source upload; `.shrike/` only; timeout | security test |
| 5.7 | GitHub Action | `planned` | Opens a draft PR on a test repo | CI log |
| 5.8 | PR template + receipt comment | `planned` | Per-batch commits; receipt link; rejected list | snapshot test |
| 5.9 | MCP wrapper | `planned` | 4 tools; no mutation | protocol test |
| 5.10 | Package naming + publish dry run | `planned` | Name available; tarball contents verified | registry |
| 5.11 | Landing test | `planned` | Two positionings; qualified sign-ups measured | report |
| 5.12 | Teardown content | `planned` | 10 public Next.js repos; before/after receipts | published post |
| 5.13 | Design-partner kit | `planned` | One-command trial; read-only first | doc |
| 5.14 | Design partners | `planned` | 3 teams; at least 1 merged PR each | tracker |
| 5.15 | Launch | `planned` | Post, demo, receipt screenshots | URL |
| 5.16 | Docs update | `planned` | `docs/07` matches behavior | review |

### Phase 6 — Hardening and scale

| ID | Task | Status | Acceptance | Evidence |
|---|---|---|---|---|
| 6.1 | Corpus expansion | `planned` | ≥ 250 must-not-report entries | corpus report |
| 6.2 | Labeled set | `planned` | 10–20 real repositories with labels | dataset |
| 6.3 | Calibration report | `planned` | Per-rule precision + calibration curve | report |
| 6.4 | Monorepo scale | `planned` | 10k+ files; workspace boundaries correct | benchmark |
| 6.5 | 10k performance | `planned` | All budgets met on reference hardware | benchmark |
| 6.6 | Security review | `planned` | Trust boundaries, redaction, injection tests | review |
| 6.7 | Supply chain | `planned` | Signed binaries, checksums, provenance, audit | release log |
| 6.8 | Failure injection | `planned` | Crash, timeout, corrupted cache, failed revert | CI log |
| 6.9 | Docs and README final | `planned` | Every claim reproducible; links valid | review |
| 6.10 | Release candidate | `planned` | 1.0.0-rc.1 on npm + GitHub | registry |
| 6.11 | 1.0 release | `planned` | Tagged, signed, published | registry |
| 6.12 | Opt-in telemetry | `deferred` | Not in v1; revisit after adoption | — |

---

## 5. Metrics

| Metric | Target | Actual | Date | Notes |
|---|---|---|---|---|
| 1,000-file cold scan | ≤ 1.5 s | TBD | — | Rust engine |
| 1,000-file warm scan | ≤ 400 ms | TBD | — | Rust engine |
| 10,000-file cold scan | ≤ 6 s | TBD | — | Rust engine |
| 10,000-file warm scan | ≤ 2 s | TBD | — | Rust engine |
| 10,000-file peak RSS | ≤ 600 MB | TBD | — | Rust engine |
| Delta (1,000 files, warm) | ≤ 1 s | TBD | — | — |
| Fast path (delta + typecheck + lint) | ≤ 60 s | TBD | — | Hook |
| Full receipt | ≤ 10 min | TBD | — | CI |
| Must-not-report false positives | 0 | TBD | — | Corpus |
| Batches verified without edits | ≥ 80% | TBD | — | Design partners |
| PRs with orphan-delta hit | ≥ 30% | TBD | — | Design partners |
| Design partners merged | ≥ 3 | TBD | — | Month one |
| Receipts with complete check record | 100% | TBD | — | Schema |

The tracker records actuals, not projections. A target is not a result.

---

## 6. Decision log

The active decisions are below. Rejected alternatives and the reasoning are in
[`docs/12-decision-log.md`](docs/12-decision-log.md).

| ID | Decision | Alternatives considered | Rationale | Status |
|---|---|---|---|---|
| D-001 | Shrike 1.0 is a greenfield app, not an upgrade | in-place upgrade; rewrite of the old prototype | Clean folder structure, no legacy constraints, one product surface | accepted |
| D-002 | Rust engine + TypeScript host | TypeScript-only; pure Rust CLI; WASM | Graph workload needs Rust; config and verifier need Node | accepted |
| D-003 | Whole-graph PR orphan delta is the wedge | current-state scan; changed-file audit | Finds code a PR killed in untouched files; incumbents cannot | accepted |
| D-004 | Verified deletion batches + bisection | advisory verdicts; one-finding-at-a-time | Higher throughput; isolates regressions; produces a PR | accepted |
| D-005 | Portable receipt is the product artifact | score; dashboard; plain report | Auditable, shareable, reproducible; not a metric trap | accepted |
| D-006 | Build graph is optional evidence | hard dependency; ignore it | Confirms bytes/routes and suppresses false positives without a version lock | accepted |
| D-007 | Next.js/React depth, not breadth | 100+ plugins; multi-language | Depth is the moat; breadth is owned | accepted |
| D-008 | No slop score | score; badge; leaderboard | Wrong metric; commoditizes into a linter | accepted |
| D-009 | PR-first, never auto-commit | auto-commit; push to main | Trust must be earned in review | accepted |
| D-010 | Fail closed on unknown edges | best-effort deletion | One false positive is a release blocker | accepted |
| D-011 | npm CLI + platform binary packages | single static binary; WASM | Natural install path; Node toolchain; native speed | accepted |
| D-012 | Deterministic, content-addressed outputs | best-effort ordering | Reproducible CI and receipts | accepted |
| D-013 | Blast-radius guard | unbounded batches | Reviewer trust; bounded risk | accepted |
| D-014 | Agent hook and MCP are distribution, not identity | MCP-first product | The verified PR is the product; surfaces are reach | accepted |
| D-015 | TypeScript/JavaScript only | multi-language | Verification is framework-specific; depth wins | accepted |
| D-016 | No hosted dashboard in v1 | dashboard; team platform | Not needed to prove the wedge; second product | accepted |
| D-017 | SCC-first cascade, deterministic batches | per-symbol closure; recursive deletion; exact coloring | A cycle is one unit; greedy coloring is deterministic and near-optimal | accepted |
| D-018 | Fail closed on protocol errors | retry and continue; best-effort parse; missing field as empty | A partial result is not a proof | accepted |

---

## 7. Risks

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Incumbent ships verified deletion + receipts | medium | high | Go deeper on orphan delta, build evidence, corpus, receipts |
| Verification is too slow for CI | high | high | Tiered checks; affected-only later; cache baseline; nightly full path |
| Next.js build snapshot changes or disappears | medium | medium | Optional evidence; version guard; static fallback |
| Orphan delta is noisy on real PRs | medium | high | Corpus; conservative unknown edges; workspace awareness; triage |
| Engine work delays the wedge | high | medium | Contract and fixtures first; timebox; ship delta and verification early |
| Buyer mismatch | medium | high | Target teams with CI; "vibe coding" is content only |
| Dirty tree or failed revert | low | high | Worktree default; snapshots; exit 5; failure drill |
| Flaky tests misclassified | high | medium | Reruns; signatures; record; no false regression |
| False positive on a framework convention | medium | high | Fixture-first; corpus gate; build evidence; review |
| Package name unavailable | medium | low | Check early; use a scope |
| Running project scripts is a trust boundary | certain | medium | Worktree; document; isolate untrusted repos |
| Rust binary distribution complexity | medium | medium | Platform packages; checksums; CI matrix |
| `oxc` API churn | medium | medium | Pin version; isolate behind `shrike-syntax` |
| Blast-radius cap suppresses useful deletions | medium | low | Configurable; report suppressed batches; revisit with data |

---

## 8. Research log

Dated entries from the research that produced this package. These are findings,
not commitments; commitments are in the plans.

### 2026-11-19 — prior prototype review

- Reviewed a prior Shrike prototype's thesis, architecture, rules, confidence
  model, agent protocol, verifier, and Next.js/React semantics.
- Lesson kept: verification is the trust layer; detection is commoditized.
- Lesson kept: fail closed on dynamic edges and missing checks.
- Lesson rejected: broad rule catalogue as the product.
- The prototype is not a code dependency for Shrike 1.0.

### 2026-11-19 — initial market scan

- Knip: broad config/plugin coverage, large distribution, no end-to-end
  verification.
- Fallow: Rust + Oxc, fast, 100+ plugins, changed-file audit, LSP/MCP/Node API,
  no verified deletion loop.
- aislop: 50+ rules, multi-language, score, hooks, MCP, CI; no baseline-backed
  deletion proof.
- Vercel `next analyze export`: exposes the Turbopack module/chunk graph; no
  source-level deletion safety.

### 2026-11-20 — competitor re-check

- **Knip:** ~86M downloads/month and 21k+ public projects observed on the
  project site; still read-only at the trust layer.
- **Fallow:** 5.4k stars; benchmark claim of 20,558 files in 2.95 s; 100+
  framework plugins; changed-file audit and auto-fix; no baseline-backed
  deletion loop.
- **aislop:** ~110k npm downloads/month; 50+ rules; score; MCP; fix; CI; safe
  mode refuses code deletion.
- **CodeTruth:** 7 stars; Python-first with a JS/TS plugin; `safe_to_delete` /
  `likely_dead` / `uncertain` / `used`; MCP; baseline; cross-repo and runtime
  evidence; claims 0 false positives on 36,457 symbols. **Direct adjacent
  competitor.**
- **Necro:** 0 stars; TypeScript/Python; confidence tiers; evidence chains;
  `test-only` verdict; MCP `verify_removal` in throwaway worktrees. **Direct
  adjacent competitor.**
- Implication: "safe deletion" alone is no longer a unique wedge. The new wedge
  is **PR-native orphan delta + verified batch PR + portable receipt + build
  evidence**, not generic safety advice.

### 2026-11-20 — technical substrate

- `oxc_parser` and `oxc_semantic` provide scopes, symbols, references, and
  unresolved references suitable for a per-file summary.
- `next analyze export` exposes Turbopack module/chunk data as JSON Lines.
- TypeScript 7's semantic API is a stdio server, not an in-process JS API; a
  TypeScript host does not get in-process type checking for free.
- Rust crates under consideration: `oxc_*`, `rayon`, `serde`, `blake3`,
  `rustc-hash`, `petgraph`, `thiserror`, `tracing`, `insta`.

### 2026-11-20 — direction reset

- The package was rewritten as a greenfield app, not an upgrade.
- The prior plan set (TypeScript-first, optional Rust extraction) was rejected.
- The active plan set is PLAN-0 through PLAN-6, with Rust as the primary engine
  from the first implementation phase.

---

## 9. Change log

| Date | Change | Files |
|---|---|---|
| 2026-11-20 | Created the greenfield Shrike 1.0 package: master prompt, docs, plans, prompts, research, schemas, folder READMEs | repository root |
| 2026-11-20 | Recorded the sharpened wedge against CodeTruth and Necro | `docs/00`, `docs/11`, `research/01` |
| 2026-11-20 | Added blast-radius guard and impact preview to the algorithm | `docs/03`, `docs/04`, `plans/PLAN-2` |
| 2026-11-20 | Added dedicated agent prompts and an adversarial red-team prompt | `prompts/` |
| 2026-11-20 | Added JSON Schemas for protocol, snapshot, delta, plan, receipt, config | `schemas/` |
| 2026-11-20 | Added schema validator and validated the design package end to end | `scripts/validate-schemas.mjs`, `TRACKER.md` |
| 2026-11-20 | Reconciled D-017/D-018 and the 0.15 acceptance with the decision log | `TRACKER.md`, `docs/12` |

---

## 10. Evidence log

A phase or deliverable is done only when evidence is linked here. Empty until
implementation starts; design-package validation is recorded below.

| Date | Phase | Evidence | Result |
|---|---|---|---|
| 2026-11-20 | Design package | `node scripts/check-links.mjs` | OK: 46 Markdown files; all relative links resolve |
| 2026-11-20 | Design package | `node scripts/check-docs.mjs` | OK: 7 plans, 111 task IDs, 18 decisions, 44 linked files; tracker is consistent |
| 2026-11-20 | Design package | `node scripts/validate-schemas.mjs` | OK: 6 schemas, 6 unique `$id` values, 35 local refs; structure valid |
| 2026-11-20 | 1.1–1.4 | `cargo test -p shrike-core -p shrike-protocol -p shrike-syntax` | OK: 13 core + 1 protocol + 6 syntax tests |
| 2026-11-20 | 2.1–2.4 | `cargo test --workspace` | OK: all Rust crates compile; graph/delta/plan/evidence/receipt tests pass |
| 2026-11-20 | 2.5 | `printf '{"protocolVersion":"1.0","id":"1","command":"ping","payload":{}}\n' \| cargo run -q -p shrike-engine` | OK: `{"ok":true,"result":{"version":"1.0.0","protocolVersion":"1.0"}}` |
| 2026-11-20 | 1.5 | `cargo check -p shrike-resolve` | OK: relative, tsconfig paths/baseUrl, workspace package, framework alias resolution |

---

## 11. How to update this tracker

1. Change the task status in §4 and, if the task is done, add a link to the
   evidence.
2. If a task ID changes, change it in the matching plan in the same commit.
3. If a metric is measured, record the actual, the date, and the machine.
4. If a decision changes, add a new row to §6 and a note in
   [`docs/12-decision-log.md`](docs/12-decision-log.md); do not rewrite history.
5. If a risk materializes, add it to §7 with the response and the evidence.
6. Keep the research log and change log current; they are the audit trail.
