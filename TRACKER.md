# Shrike 1.0 — Tracker

> **Canonical design:** [`MASTER_PROMPT.md`](MASTER_PROMPT.md)
> **Status:** implementation complete for phases 0–4 and the local distribution/hardening surfaces; external launch, design-partner, publish, and real-app acceptance items are blocked
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
- **Implementation:** phases 0–4 are complete and exercised by the Rust,
  Node, fixture, corpus, and prune e2e gates. The CLI, verifier, receipts,
  hooks, action, and MCP server are implemented. Phase 4 (Next.js build
  evidence) includes bounded-memory JSONL streaming, module-ID and suffix
  matching, per-candidate client/server verdicts, unknown-size accounting,
  server-action/client-reference build edges, the client-boundary leak
  diagnostic, stale/build-failed statuses, receipt population,
  `--require-build-evidence`, and build-edge feedback into reachability. The
  must-not-report corpus runs every case as its own project in both static and
  build-evidence modes from a clean cache (250 cases, zero protected symbols
  reported), and eleven `fixtures/build-graph/` scenarios cover the join,
  fallback, and boundary matrix. External launch, design-partner, publish, and
  real-app acceptance items are `blocked` on credentials or participants, not on
  code.
- **Prior prototype:** not a dependency. The old tree was removed; a backup
  exists outside the repository only as a safety net. No code, schema, or fixture
  from it is reused.

---

## 3. Phase overview

| Phase | Plan | Status | Exit gate |
|---|---|---|---|
| 0 | [Foundations](plans/PLAN-0-foundations.md) | `done` | Contract, schemas, fixtures, CI green |
| 1 | [Rust engine](plans/PLAN-1-rust-engine.md) | `done` | Engine builds, fixtures pass, budgets met |
| 2 | [Graph and delta](plans/PLAN-2-graph-and-delta.md) | `done` | Orphan delta correct; corpus clean |
| 3 | [Verification and receipts](plans/PLAN-3-verification-and-receipts.md) | `done` | End-to-end verified deletion PR + receipt |
| 4 | [Build graph and Next.js](plans/PLAN-4-build-graph-and-nextjs.md) | `done` | Build evidence join, streaming, verdicts, bytes, boundaries, fallbacks, fixtures, and receipts green; real-app coverage is a release gate |
| 5 | [Distribution and launch](plans/PLAN-5-distribution-and-launch.md) | `in-progress` | Hooks + Action + MCP done; design partners and publish blocked |
| 6 | [Hardening and scale](plans/PLAN-6-hardening-and-scale.md) | `in-progress` | Corpus, scale, benchmarks done; labeled set and release blocked |

---

## 4. Tasks

### Phase 0 — Foundations

| ID | Task | Status | Acceptance | Evidence |
|---|---|---|---|---|
| 0.1 | Confirm greenfield layout | `done` | Root `crates/`, `packages/`, `schemas/`, `fixtures/`, `benchmarks/`, `scripts/` named; no legacy dependency | `crates/`, `packages/`, `schemas/`, `fixtures/`, `benchmarks/`, `scripts/` |
| 0.2 | Write engine/host protocol | `done` | Versioned JSON; valid, malformed, timeout, crash, partial-result cases | `crates/shrike-protocol/`; `packages/cli/test/contracts.test.mjs` |
| 0.3 | Write `GraphSnapshot` schema | `done` | Files, symbols, modules, entries, edges, certainty, hashes, version | `schemas/graph-snapshot.schema.json`; `scripts/check-schemas.mjs` |
| 0.4 | Write `Delta` schema | `done` | Newly dead symbols/files; witness; trigger; cascade; batches | `schemas/delta.schema.json`; `fixtures/delta/` |
| 0.5 | Write `DeletionPlan` schema | `done` | Batches, impact, blast radius, patches | `schemas/deletion-plan.schema.json`; `fixtures/delta/` |
| 0.6 | Write `Receipt` schema | `done` | Batch, checks, baseline status, outcome, rejected, environment | `schemas/receipt.schema.json`; `fixtures/receipts/` |
| 0.7 | Write `Config` schema | `done` | Include/ignore, entries, checks, policy, build evidence | `schemas/config.schema.json`; `packages/cli/src/config.ts` |
| 0.8 | Scaffold Rust workspace | `done` | `cargo test`, `cargo clippy`, `cargo fmt --check` green | `cargo fmt --check`; `cargo clippy --workspace --all-targets`; `cargo test --workspace` |
| 0.9 | Scaffold TypeScript packages | `done` | `pnpm test`, `pnpm typecheck`, `pnpm lint` green | `pnpm -r typecheck`; `pnpm test:node`; `node scripts/lint.mjs` |
| 0.10 | Build fixture harness | `done` | Temp repos, fixture checkout, snapshot assertions | `packages/testkit/`; `scripts/e2e-prune.mjs` |
| 0.11 | Build must-not-report corpus | `done` | ≥ 100 entries from framework docs and real apps | `fixtures/must-not-report/` (250 entries, 21 categories) |
| 0.12 | Build delta fixtures | `done` | Nine scenarios from `docs/04` §11 | `fixtures/delta/` (14 scenarios); `node scripts/delta-fixtures.mjs` |
| 0.13 | Build verification fixtures | `done` | Apply, bisect, revert, flaky, known-failure | `fixtures/verification/` (6 scenarios); `node scripts/verify-fixtures.mjs` |
| 0.14 | Wire CI | `done` | Rust, Node, schemas, corpus, fixtures, bench smoke | `.github/workflows/ci.yml`; `.github/workflows/nightly.yml` |
| 0.15 | Record decisions and tracker | `done` | D-001…D-018 recorded; phase tables match plans | this file §6; `docs/12-decision-log.md` |

### Phase 1 — Rust engine

| ID | Task | Status | Acceptance | Evidence |
|---|---|---|---|---|
| 1.1 | `shrike-core`: IDs, interners, spans, errors | `done` | Deterministic IDs and stable serialization | `crates/shrike-core/src/`; `cargo test -p shrike-core` |
| 1.2 | `shrike-protocol`: envelope and codec | `done` | Line-delimited JSON; versioned; malformed fails closed | `crates/shrike-protocol/src/`; `cargo test -p shrike-protocol` |
| 1.3 | `shrike-syntax`: parse + per-file summary | `done` | Imports, exports, re-exports, JSX, directives, dynamic forms | `crates/shrike-syntax/src/`; `cargo test -p shrike-syntax` |
| 1.4 | `shrike-syntax`: semantic scopes/symbols/references | `done` | Binding, declaration, and reference facts captured with `oxc_parser`/AST; no AST retention | `crates/shrike-syntax/src/`; `cargo test -p shrike-syntax` |
| 1.5 | `shrike-resolve`: resolver | `done` | Relative, alias, workspace, package exports, barrel; unresolved recorded | `crates/shrike-resolve/src/`; resolver fixtures |
| 1.6 | `shrike-graph`: snapshot + module graph | `done` | Files, modules, symbols, edges, entries, hashes | `crates/shrike-graph/src/`; `cargo test -p shrike-graph` |
| 1.7 | Entry adapters: Next.js App Router | `done` | Pages, layouts, routes, metadata, server actions, client boundaries | `crates/shrike-graph/src/entries.rs`; `fixtures/delta/removed-route` |
| 1.8 | Entry adapters: Pages Router + React | `done` | Pages, API routes, `_app`, `_document`, data functions, React roots | `crates/shrike-graph/src/entries.rs`; pages/react fixtures |
| 1.9 | Reachability + witnesses | `done` | Every live symbol has a shortest witness; every dead symbol has a proof trace | `crates/shrike-graph/src/`; witness/proof tests |
| 1.10 | Dynamic-edge abstraction | `done` | Literal, template, glob, namespace, string-keyed, unknown | `crates/shrike-syntax/src/`; `fixtures/delta/dynamic` |
| 1.11 | SCC condensation | `done` | Cycles treated as units; no partial-cycle deletion | `crates/shrike-graph/src/`; `fixtures/delta/cycle` |
| 1.12 | Snapshot cache + warm path | `done` | Content-addressed; warm scan skips unchanged files; atomic writes | `crates/shrike-engine/src/lib.rs`; `benchmarks/harness.mjs` |
| 1.13 | `shrike-engine` binary + commands | `done` | `snapshot`, `delta`, `plan`, `proof`, `evidence`, `version` | `crates/shrike-engine/src/lib.rs`; `packages/cli/src/engine.ts` |
| 1.14 | Determinism harness | `done` | Two runs and different thread counts produce byte-identical snapshots | `.github/workflows/ci.yml` determinism step; `benchmarks/harness.mjs` |
| 1.15 | Performance harness | `done` | 1k/10k cold, warm, peak RSS recorded against budgets | `benchmarks/README.md`; this file §5 |
| 1.16 | Fail-closed errors | `done` | Missing binary, malformed response, timeout, crash all exit 4 | `packages/cli/test/contracts.test.mjs` (missing, malformed, crash, timeout) |
| 1.17 | CLI integration: `scan`, `proof` | `done` | Output matches fixture oracle; exit codes correct | `packages/cli/src/main.ts`; `scripts/e2e-prune.mjs` |

### Phase 2 — Graph and delta

| ID | Task | Status | Acceptance | Evidence |
|---|---|---|---|---|
| 2.1 | Base snapshot acquisition | `done` | Merge-base worktree or archive; read-only; cached | `packages/cli/src/main.ts` `analyzePair`; `scripts/e2e-prune.mjs` |
| 2.2 | Head snapshot | `done` | Commit or dirty tree; `dirty` flag | `packages/cli/src/main.ts`; `fixtures/delta/dirty` |
| 2.3 | Comparability gate | `done` | Refuses incomparable inputs with a named reason | `crates/shrike-delta/src/`; `fixtures/delta/` |
| 2.4 | Live-set diff | `done` | `live(base) − live(head)`; no pre-existing debt | `crates/shrike-delta/src/`; `node scripts/delta-fixtures.mjs` |
| 2.5 | Witness lookup | `done` | Lost witness path present for every delta item | `crates/shrike-graph/src/`; `shrike proof` |
| 2.6 | Trigger attribution | `done` | Names the changed file/symbol that removed the edge | `crates/shrike-delta/src/`; delta fixture oracles |
| 2.7 | Cascade closure | `done` | Exclusive ownership; stop conditions; unknown blocks | `crates/shrike-delta/src/`; `fixtures/delta/` |
| 2.8 | Batch conflict graph | `done` | No shared files/SCC/order within a batch | `crates/shrike-plan/src/`; plan tests |
| 2.9 | Blast-radius guard | `done` | Batch over limit is split or refused with a reason | `crates/shrike-plan/src/`; batch cap tests |
| 2.10 | Impact estimation | `done` | Files, lines, and symbols reported; route/byte impact available | delta/plan report merged-span lines, files, and symbols; `shrike evidence` reports route/shared/matched bytes and `prune` receipts record the build method/hash; `fixtures/build-graph/` and `packages/cli/test/build-evidence.test.mjs` |
| 2.11 | `shrike delta` | `done` | JSON + human output; schema-valid | `packages/cli/src/main.ts`; `scripts/e2e-prune.mjs` |
| 2.12 | `shrike plan` | `done` | Batches with impact and blast radius | `packages/cli/src/main.ts`; `scripts/e2e-prune.mjs` |
| 2.13 | Delta fixtures + corpus | `done` | Nine scenarios; zero corpus findings | `node scripts/delta-fixtures.mjs`; `node scripts/corpus-runner.mjs` |
| 2.14 | Monorepo handling | `done` | Workspace boundaries respected | `crates/shrike-resolve/src/`; `fixtures/delta/monorepo` |
| 2.15 | Determinism | `done` | Byte-identical output across runs and thread counts | `node scripts/delta-fixtures.mjs`; determinism CI step |
| 2.16 | Real-app delta | `blocked` | A synthetic feature removal produces untouched-file orphans | blocked: needs a real application/PR; synthetic removal is covered by `fixtures/delta/removed-caller` |
| 2.17 | Docs update | `done` | `docs/04` matches behavior | `docs/04-whole-graph-delta.md`; this file |

### Phase 3 — Verification and receipts

| ID | Task | Status | Acceptance | Evidence |
|---|---|---|---|---|
| 3.1 | Verifier package scaffold | `done` | Public API, errors, config | `packages/verifier/src/`; `cargo test --workspace` |
| 3.2 | Baseline capture | `done` | Commit, checks, signatures, environment recorded | `packages/verifier/src/checks.ts`; `scripts/e2e-prune.mjs` |
| 3.3 | Check discovery | `done` | `package.json` scripts + config; explicit commands win | `packages/verifier/src/checks.ts`; `packages/cli/test/contracts.test.mjs` |
| 3.4 | Worktree mode | `done` | Create, apply, verify, remove; clean tree required | `packages/cli/src/git.ts`; `scripts/e2e-prune.mjs` |
| 3.5 | In-place mode | `done` | Snapshot, apply, verify, restore; dirty-tree refusal | `packages/cli/src/main.ts` (`--in-place`); prune clean-tree guard |
| 3.6 | Compare/classification | `done` | pass, improvement, known-failure, new-failure, fail, flaky, timeout | `packages/verifier/src/compare.ts`; verification fixtures |
| 3.7 | Batch apply | `done` | Independent batches verified separately | `packages/cli/src/main.ts`; `scripts/e2e-prune.mjs` |
| 3.8 | Bisection | `done` | Minimal failing set isolated and reverted | `packages/verifier/src/bisect.ts`; `scripts/e2e-prune.mjs` bisected batch |
| 3.9 | Revert/recovery | `done` | Clean restore or exit 5 with recovery path | `packages/cli/src/recovery.ts`; exit-5 e2e scenario |
| 3.10 | Receipt writer | `done` | Schema-valid; content-addressed; reproducible | `packages/receipts/src/receipt.ts`; `fixtures/receipts/` |
| 3.11 | Receipt validation | `done` | Every fixture receipt validates | `packages/receipts/test/validate.test.mjs`; `node scripts/check-receipts.mjs` |
| 3.12 | PR renderer | `done` | Markdown summary, per-batch table, rejected list | `packages/receipts/src/render.ts`; PR body/comment renderers |
| 3.13 | Per-batch commits + draft PR | `done` | One commit per verified batch; draft by default | `packages/cli/src/main.ts`; `scripts/e2e-prune.mjs` |
| 3.14 | Exit codes | `done` | 0/1/2/3/4/5 covered | `packages/cli/test/contracts.test.mjs`; `scripts/e2e-prune.mjs` (0/1/2/3/4/5) |
| 3.15 | Flaky handling | `done` | Rerun, classify, record; no false regression | `packages/verifier/src/bisect.ts`; verification flaky fixture |
| 3.16 | Failure drill | `done` | Broken deletion reverted; tree clean | `scripts/e2e-prune.mjs` rejected/revert-failure scenarios |
| 3.17 | Real-app end-to-end | `blocked` | 3 batches verified with typecheck, lint, test, build | blocked: needs a real app and three verified batches; synthetic e2e is green |
| 3.18 | Docs update | `done` | `docs/05` matches behavior | `docs/05-verified-deletion.md`; this file |

### Phase 4 — Build graph and Next.js

| ID | Task | Status | Acceptance | Evidence |
|---|---|---|---|---|
| 4.1 | Snapshot discovery | `done` | Finds the snapshot; runs export only with `--build` | `packages/cli/src/build.ts` discovers `.next/analyze`, `.shrike/build`, and `next-analyze`, or reads `--build <path>`; `packages/cli/test/build-evidence.test.mjs` |
| 4.2 | Version guard | `done` | 16.4+ full; 16.1–16.3 partial; older unsupported | `crates/shrike-evidence/src/lib.rs` capability/status tests; lower package/evidence version wins |
| 4.3 | JSONL parser | `done` | Streaming; tolerates additive fields | `packages/cli/src/build.ts` streams JSONL in 64 KiB chunks with a `StringDecoder`; object/array and JSONL inputs normalize; malformed lines fail closed with line numbers; unknown record families are counted; 10,000-line host test and `fixtures/build-graph/` |
| 4.4 | Module index | `done` | Path and ID lookup; normalized | loader prefixes, query strings, `turbopack://`, `file://`, absolute, and relative paths normalize; module IDs map to paths; exact path/symbol match then longest suffix; duplicate evidence keys are deterministic; host and Rust tests |
| 4.5 | Module-to-file join | `done` | ≥ 90% coverage on a real app; unresolved recorded | coverage thresholds `full` ≥ 0.95, `partial` ≥ 0.80, `low-coverage` < 0.80; unmatched listed; `fixtures/build/` |
| 4.6 | Presence/absence | `done` | Client/server distinction; suppress on presence | `candidate_verdicts` reports `present`, `server-only`, `absent`, `unknown`, `unsupported`, and `stale` per candidate; client-route presence beats a server layer; `fixtures/build-graph/server-action/` and `client-boundary/` |
| 4.7 | Route attribution | `done` | Per-route membership; shared chunks attributed once | per-route files/symbols/bytes in `fixtures/build/manifest.json`; shared modules counted once in `bytes.shared` |
| 4.8 | Byte impact | `done` | Per-route and total; no double counting | `matched`/`shared`/`totalRoute` are reported; missing sizes increment `bytes.unknown`, set `bytes.partial`, and are listed in `unknownSizes`; non-full joins set `bytes.partial`; `fixtures/build-graph/shared-chunk/` and host/Rust tests |
| 4.9 | Build-edge feedback | `done` | Adds certain edges; suppresses false positives | `augment_snapshot` seeds build-live modules/symbols and follows known edges; `augment-build` engine command; Rust and CLI closure tests |
| 4.10 | Server actions | `done` | Client-reference manifest considered; conservative fallback | `client-reference` records create `kind: "build"`, `certainty: "certain"` edges, seed liveness, and produce `server-only` verdicts; `fixtures/build-graph/server-action/` and Rust/host tests |
| 4.11 | Client boundaries | `done` | `"use client"` entry semantics; leak diagnostic | static `use client` semantics plus `client-boundary-leak` warning for server-layer files in client routes; opt out with `clientBoundaryLeaks: false` or `--no-client-boundary-leaks`; `fixtures/build-graph/client-boundary/` |
| 4.12 | Proxy/middleware/metadata | `done` | Entry and matcher semantics; never dead | `crates/shrike-graph/src/entries.rs`; corpus proxy/middleware/metadata cases |
| 4.13 | Fallback matrix | `done` | Missing/stale/parse-error/unsupported → static-only with status | `full`, `partial`, `low-coverage`, `none`, `unsupported`, `malformed`, `stale`, and `build-failed` are explicit; `--run-build` records failures; missing explicit paths exit 4; `--require-build-evidence` fails closed; `fixtures/build-graph/{missing,stale,malformed}/` |
| 4.14 | Receipt integration | `done` | Build evidence status and hash recorded | every prune receipt carries source, snapshot hash, version, and status; `absent` is explicit; `--require-build-evidence` fails closed; `packages/cli/test/build-evidence.test.mjs` |
| 4.15 | Corpus with/without evidence | `done` | Green in both modes | `scripts/corpus-runner.mjs` analyzes all 250 cases per-project in static and `augment-build` modes from a clean cache; 0 protected symbols reported and 8/8 remaining candidates suppressed by evidence |
| 4.16 | Docs update | `done` | `docs/06` matches behavior | `docs/06-build-graph-and-nextjs.md` implementation status, fallback matrix, fixtures, and acceptance checklist updated for streaming, verdicts, unknown sizes, stale/build-failed, server actions, and client boundaries |

### Phase 5 — Distribution and launch

| ID | Task | Status | Acceptance | Evidence |
|---|---|---|---|---|
| 5.1 | Claude Code hook | `done` | Install, run, uninstall; idempotent | `packages/agent/src/hooks.ts`; `packages/cli/test/contracts.test.mjs` |
| 5.2 | Cursor hook | `done` | Same fast path | `packages/agent/src/hooks.ts`; `packages/cli/test/contracts.test.mjs` |
| 5.3 | OpenCode hook | `done` | Same fast path | `packages/agent/src/hooks.ts`; `packages/cli/test/contracts.test.mjs` |
| 5.4 | Fast-path budget | `done` | ≤ 60 s or delta-only | `shrike hook run` 55 s timeout; `packages/cli/src/main.ts` |
| 5.5 | Hook output + brief | `done` | One-screen summary + JSON `CleanupBrief` | `packages/agent/src/brief.ts`; `.shrike/brief.json` contract test |
| 5.6 | Hook safety | `done` | No source upload; `.shrike/` only; timeout | hook writes `.shrike/` only; prompt-injection sanitization; contract test |
| 5.7 | GitHub Action | `blocked` | Opens a draft PR on a test repo | `packages/action/` supports delta/prune, records outputs, and refuses forks; local tests cover delta thresholds, fork refusal, prune outputs, and unknown modes; external test-repo run is blocked on a test repository |
| 5.8 | PR template + receipt comment | `done` | Per-batch commits; receipt link; rejected list | `packages/receipts/src/render.ts`; `scripts/e2e-prune.mjs` per-batch commits |
| 5.9 | MCP wrapper | `done` | 4 tools; no mutation | `packages/cli/src/main.ts` MCP server; four-tool contract test |
| 5.10 | Package naming + publish dry run | `blocked` | Name available; tarball contents verified | local tarball gate passes (`scripts/check-package.mjs`: launcher, dist, README, license; no source/tests/fixtures); npm scope availability and registry publish dry run remain blocked on credentials |
| 5.11 | Landing test | `blocked` | Two positionings; qualified sign-ups measured | blocked: landing test requires participants |
| 5.12 | Teardown content | `blocked` | 10 public Next.js repos; before/after receipts | blocked: public repo teardown requires publishing and review |
| 5.13 | Design-partner kit | `done` | One-command trial; read-only first | `design-partner/` ships a read-only `trial.sh` (delta + plan), trust-boundary guide, triage template, and one-command removal; exercised on a synthetic feature branch |
| 5.14 | Design partners | `blocked` | 3 teams; at least 1 merged PR each | blocked: requires design partners |
| 5.15 | Launch | `blocked` | Post, demo, receipt screenshots | blocked: requires launch channel and demo assets |
| 5.16 | Docs update | `done` | `docs/07` matches behavior | `docs/07` updated for actual Action inputs/outputs, hook run, launcher resolution, packaging gate, design-partner kit, and local acceptance evidence |

### Phase 6 — Hardening and scale

| ID | Task | Status | Acceptance | Evidence |
|---|---|---|---|---|
| 6.1 | Corpus expansion | `done` | ≥ 250 must-not-report entries | `fixtures/must-not-report/` (250 entries, 21 categories) |
| 6.2 | Labeled set | `blocked` | 10–20 real repositories with labels | blocked: needs 10–20 labeled real repositories |
| 6.3 | Calibration report | `blocked` | Per-rule precision + calibration curve | blocked: calibration needs the labeled set |
| 6.4 | Monorepo scale | `done` | 10k+ files; workspace boundaries correct | `benchmarks/harness.mjs` 10k run; monorepo fixture |
| 6.5 | 10k performance | `done` | All budgets met on reference hardware | this file §5; `benchmarks/README.md` |
| 6.6 | Security review | `done` | Trust boundaries, redaction, injection tests | Internal review in `docs/09` §11; config and verifier child environments are allowlisted; redaction, prompt-injection, render, traversal, fork, dirty-tree, and cache-corruption tests pass; `pnpm audit --prod` clean and `cargo audit` 0 vulnerabilities (1550 advisories, 108 deps); external audit and signed release/launcher checksum remain blocked under 6.7 |
| 6.7 | Supply chain | `blocked` | Signed binaries, checksums, provenance, audit | blocked: signed binaries and provenance require release infrastructure |
| 6.8 | Failure injection | `done` | Crash, timeout, corrupted cache, failed revert | engine crash/timeout tests; corrupted snapshot and manifest are discarded and rebuilt; `cache verify` reports broken entries; revert-failure/recovery e2e; `packages/cli/test/contracts.test.mjs` |
| 6.9 | Docs and README final | `done` | Every claim reproducible; links valid | README status, build-graph row, quick start, and MIT license reconciled with the implementation; `node scripts/check-links.mjs` (115 files), `node scripts/check-docs.mjs` (13 docs, 7 plans, 111 task IDs), and `node scripts/check-schemas.mjs` (1231 fixture files) pass |
| 6.10 | Release candidate | `blocked` | 1.0.0-rc.1 on npm + GitHub | blocked: npm release requires credentials |
| 6.11 | 1.0 release | `blocked` | Tagged, signed, published | blocked: release requires credentials and external validation |
| 6.12 | Opt-in telemetry | `deferred` | Not in v1; revisit after adoption | deferred by D-016 |

---

## 5. Metrics

| Metric | Target | Actual | Date | Notes |
|---|---|---|---|---|
| 1,000-file cold scan | ≤ 1.5 s | 222 ms | 2026-11-20 | release engine; 7-run median; AMD Ryzen 7 5700U, WSL2 |
| 1,000-file warm scan | ≤ 400 ms | 146 ms (178 ms p95) | 2026-11-20 | cache hit; 7 measured runs; byte-identical snapshot |
| 10,000-file cold scan | ≤ 6 s | 2.70 s | 2026-11-20 | release engine; 10,001 files; 7-run median |
| 10,000-file warm scan | ≤ 2 s | 1.37 s (1.47 s p95) | 2026-11-20 | cache hit; 22,018 symbols |
| 10,000-file peak RSS | ≤ 600 MB | 231 MB | 2026-11-20 | harness `/proc/<pid>/status` VmHWM |
| Delta (1,000 files, warm) | ≤ 1 s | 346 ms (408 ms p95) | 2026-11-20 | base/head snapshots; 1 newly dead symbol; 7 measured runs |
| Fast path (delta + typecheck + lint) | ≤ 60 s | 55 s hard timeout | 2026-11-20 | `shrike hook run`; no real-app timing yet |
| Full receipt | ≤ 10 min | not measured | — | requires a real app |
| Must-not-report false positives | 0 | 0 | 2026-11-20 | 250 entries; 8 candidates, 0 protected symbols |
| Batches verified without edits | ≥ 80% | 1/1 synthetic | 2026-11-20 | e2e verified batch; no partner data |
| PRs with orphan-delta hit | ≥ 30% | no real sample | — | blocked on labeled set |
| Design partners merged | ≥ 3 | 0 | 2026-11-20 | blocked: no partners yet |
| Receipts with complete check record | 100% | 100% of fixture suite | 2026-11-20 | verified receipts require at least one check |

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
| 2026-11-20 | Implemented the Rust engine, TypeScript host, CLI, verifier, receipts, hooks, action, MCP server, schemas, fixtures, benchmarks, and CI | `crates/`, `packages/`, `schemas/`, `fixtures/`, `benchmarks/`, `.github/` |
| 2026-11-20 | Added content-addressed recovery snapshots, `shrike recover`, exit 5, and revert-failure receipts | `packages/cli/src/recovery.ts`, `scripts/e2e-prune.mjs` |
| 2026-11-20 | Added engine timeout, config child-process isolation, four-tool MCP contract, and receipt structural validation | `packages/cli/src/engine.ts`, `packages/cli/src/config.ts`, `packages/receipts/src/validate.ts` |
| 2026-11-20 | Expanded the must-not-report corpus to 250 entries and recorded benchmark actuals | `fixtures/must-not-report/`, `TRACKER.md` |
| 2026-11-20 | Implemented build-evidence discovery, JSONL/object ingestion, version guard, join coverage, route/byte attribution, and receipt population | `packages/cli/src/build.ts`, `crates/shrike-evidence/`, `fixtures/build/`, `packages/cli/test/build-evidence.test.mjs` |
| 2026-11-20 | Added build-edge feedback: build-live modules seed a liveness closure over known edges; build evidence can only suppress, never delete | `crates/shrike-evidence/`, `crates/shrike-engine/`, `packages/cli/src/main.ts` |
| 2026-11-20 | Fixed root-package self-reference resolution and dynamic imports inside function bodies; made snapshot caches resolver-aware; corpus runner now analyzes each case independently in static and evidence modes | `crates/shrike-resolve/`, `crates/shrike-syntax/`, `crates/shrike-engine/`, `scripts/corpus-runner.mjs` |
| 2026-11-20 | Completed the local security review: allowlisted config and check environments, added prompt-injection and render tests, recorded audits, and documented residual release gates | `packages/verifier/src/run.ts`, `packages/cli/src/config.ts`, `packages/agent/test/`, `packages/receipts/test/`, `docs/09-security-and-trust.md`, `.github/workflows/ci.yml` |
| 2026-11-20 | Added a warm delta measurement and p95 reporting to the benchmark harness; recorded 1k/10k actuals | `benchmarks/harness.mjs`, `benchmarks/README.md`, `TRACKER.md` |
| 2026-11-20 | Reconciled README, license, and docs with the implemented build-graph and distribution state | `README.md`, `LICENSE`, `packages/cli/package.json`, `docs/08-benchmarks-and-confidence.md` |

---

## 10. Evidence log

A phase or deliverable is done only when evidence is linked here. Empty until
implementation starts; design-package validation is recorded below.

| Date | Phase | Evidence | Result |
|---|---|---|---|
| 2026-11-20 | Design package | `node scripts/check-links.mjs` | OK: 111 Markdown files; all relative links resolve |
| 2026-11-20 | Design package | `node scripts/check-docs.mjs` | OK: 13 docs, 7 plans, 111 task IDs; tracker is consistent |
| 2026-11-20 | Design package | `node scripts/validate-schemas.mjs` | OK: 6 schemas, 6 unique `$id` values; structure valid |
| 2026-11-20 | 1.1–1.17 | `cargo test --workspace` | OK: all Rust crate tests pass |
| 2026-11-20 | 0.9, 3.1–3.16 | `pnpm -r typecheck` | OK: agent, action, receipts, testkit, verifier, cli |
| 2026-11-20 | 0.9, 3.14 | `pnpm test:node` | OK: 28 Node tests (engine failures, MCP, hooks, recovery, receipts) |
| 2026-11-20 | 0.8 | `cargo fmt --check`; `cargo clippy --workspace --all-targets -- -D warnings` | OK |
| 2026-11-20 | 0.11, 2.13, 4.15 | `node scripts/check-corpus.mjs`; `node scripts/corpus-runner.mjs` | OK: 250 entries; 0 protected symbols in 8 candidates; evidence mode suppressed 8/8 candidates from a clean cache |
| 2026-11-20 | 0.12, 2.13 | `node scripts/delta-fixtures.mjs` | OK: 14 delta scenarios |
| 2026-11-20 | 0.13, 3.6 | `node scripts/verify-fixtures.mjs` | OK: 6 verification scenarios |
| 2026-11-20 | 3.10–3.11 | `node scripts/check-receipts.mjs` | OK: 3 generated receipts, 2 schema examples, committed/local receipts |
| 2026-11-20 | 3.4–3.16 | `node scripts/e2e-prune.mjs` | OK: verified, rejected, bisected, revert-failure/recovery |
| 2026-11-20 | 1.15, 6.4–6.5 | `node benchmarks/harness.mjs` | OK: 1k/10k budgets met; deterministic snapshots |
| 2026-11-20 | 0.14 | `node scripts/check.mjs` | OK: schemas, receipts, corpus, delta, verification, build, prune e2e |
| 2026-11-20 | 4.1–4.16 | `node --test packages/cli/test/build-evidence.test.mjs` | OK: 10 tests — discovery, explicit path, JSONL/object join, malformed/traversal refusal, version guard, required evidence, suppression, edge closure, receipt metadata, fail-closed prune |
| 2026-11-20 | 4.9 | `cargo test -p shrike-evidence` | OK: 7 tests, including build liveness closure and low-coverage non-augmentation |
| 2026-11-20 | 4.1–4.16 | `node scripts/check.mjs` | OK: build fixture joins `low-coverage`, coverage 0.714, 2,600 matched bytes; all gates green |
| 2026-11-20 | 0.8, 3.14 | `cargo fmt --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `pnpm -r typecheck`; `pnpm test:node` | OK: 38 Node tests; no clippy warnings |
| 2026-11-20 | 2.4, 4.15, 6.8 | `cargo test -p shrike-resolve`; `node scripts/corpus-runner.mjs`; `node scripts/check.mjs` | OK: root-package self-reference regression test; 250/250 corpus cases green in both modes; all local gates green |
| 2026-11-20 | 4.3, 4.4, 4.6, 4.8, 4.10, 4.11, 4.13 | `node --test packages/cli/test/build-evidence.test.mjs` | OK: 16 tests — streaming/module IDs/layers/client references, suffix matching, stale, build-failed, unknown sizes/leak opt-out, 10k-line JSONL, fail-closed prune |
| 2026-11-20 | 4.2, 4.6, 4.8, 4.10, 4.11, 4.13 | `cargo test -p shrike-evidence` | OK: 13 tests — joins, verdicts, shared bytes, unknown sizes, leaks, client-reference liveness, stale/build-failed |
| 2026-11-20 | 4.1–4.16 | `node scripts/run-fixtures.mjs` | OK: 14 delta, 6 verification, 1 build, 11 build-graph scenarios; all joins and fallbacks pass |
| 2026-11-20 | 0.8, 0.9 | `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace` | OK: all Rust tests pass; no clippy warnings |
| 2026-11-20 | 0.9, 5.1–5.9, 6.6 | `pnpm -r typecheck`; `pnpm test:node` | OK: typecheck green; 73 Node tests pass (agent, action, build evidence, CLI contracts, hooks, receipts, recovery, verifier) |
| 2026-11-20 | 0.11, 0.12, 0.13, 2.13, 3.4–3.16, 4.1–4.16, 6.8 | `node scripts/check.mjs` | OK: schemas/receipts/package/corpus/14 delta/6 verification/1 build/11 build-graph/prune e2e all green |
| 2026-11-20 | 6.6 | `pnpm audit --prod`; `cargo audit --json` | OK: pnpm no known vulnerabilities; cargo audit 0 vulnerabilities (1550 advisories, 108 dependencies) |
| 2026-11-20 | 6.6, 5.5, 5.6 | `node --test packages/agent/test/brief.test.mjs`; `node --test packages/receipts/test/redact.test.mjs`; `node --test packages/receipts/test/render.test.mjs`; `node --test packages/verifier/test/checks.test.mjs` | OK: injection stripping/labels/caps, synthetic secret redaction, Markdown escaping, safe check environment |
| 2026-11-20 | 6.5, 6.9 | `node benchmarks/harness.mjs --files 1000 --runs 7`; `node benchmarks/harness.mjs --files 10000 --runs 7`; `node scripts/check-links.mjs`; `node scripts/check-docs.mjs`; `node scripts/check-schemas.mjs` | OK: 1k 222/146 ms cold/warm, 346 ms delta; 10k 2.70/1.37 s cold/warm, 231 MB RSS; 115 links, 13 docs/7 plans/111 IDs, 1231 fixture files |

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
