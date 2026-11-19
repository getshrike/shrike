# PLAN-0 — Foundations

> **Parent:** [MASTER_PROMPT.md](../MASTER_PROMPT.md) · **Tracker:** [TRACKER.md](../TRACKER.md)
> **Status:** `planned` · **Depends on:** — · **Unblocks:** PLAN-1
> **Owner prompt:** [`../prompts/00-orchestrator.md`](../prompts/00-orchestrator.md)

---

## 1. Goal

Freeze the contract, schemas, fixtures, and CI before the Rust engine is built.
The point is to make the later phases mechanical: the engine/host contract, the
snapshot model, the receipt, and the must-not-report corpus are the contract.

This plan is deliberately boring. It prevents the two classic failures:

- the engine and the verifier drift because the contract was invented twice;
- the corpus is assembled after the rules, so it encodes the rules' bugs.

This is a greenfield repository. Every directory below is created here for the
first time. There is no legacy layout to preserve.

---

## 2. Scope

### In scope

- A versioned engine/host JSON contract over stdin/stdout.
- `GraphSnapshot`, `Delta`, `DeletionPlan`, `Receipt`, and `Config` schemas.
- A fixture harness and the first fixtures.
- The must-not-report corpus (≥ 100 entries at this phase; ≥ 120 by PLAN-6).
- The Rust workspace and TypeScript package scaffolds.
- CI, link checking, schema validation, and plan/tracker consistency.
- Decision records and the tracker.

### Out of scope

- Parser, resolver, graph, delta, or verification logic.
- Real Next.js build-graph ingestion.
- Distribution, hooks, Action, or MCP.
- Performance claims.

---

## 3. Deliverables

```text
.
├── Cargo.toml                     # Rust workspace
├── rust-toolchain.toml
├── package.json                   # private root, pnpm scripts
├── pnpm-workspace.yaml
├── tsconfig.base.json
├── .editorconfig
├── .gitignore
├── .github/workflows/ci.yml
├── schemas/
│   ├── protocol.schema.json
│   ├── graph-snapshot.schema.json
│   ├── delta.schema.json
│   ├── deletion-plan.schema.json
│   ├── receipt.schema.json
│   └── config.schema.json
├── crates/
│   ├── shrike-core/
│   ├── shrike-protocol/
│   └── shrike-engine/
├── packages/
│   ├── cli/
│   ├── testkit/
│   └── README.md
├── fixtures/
│   ├── README.md
│   ├── next-app-router/
│   ├── next-pages-router/
│   ├── dynamic-edges/
│   └── must-not-report/
├── benchmarks/
│   └── README.md
└── scripts/
    ├── check-links.mjs
    ├── check-docs.mjs
    └── validate-schemas.mjs
```

---

## 4. Work breakdown

### 0.1 — Confirm greenfield layout

**Task:** Create the top-level directories and root manifests named in
[`../docs/02-folder-structure.md`](../docs/02-folder-structure.md). Record that
no legacy dependency exists.

**Acceptance:**

- `crates/`, `packages/`, `schemas/`, `fixtures/`, `benchmarks/`, and `scripts/`
  exist with the documented README.
- No path references a prior prototype, cache format, or package name.
- `docs/02-folder-structure.md` matches the tree.

**Evidence:** tree listing in the tracker.

### 0.2 — Write the engine/host protocol

**Task:** Define the request/response envelope and the five commands used by the
host: `snapshot`, `delta`, `plan`, `proof`, `evidence`.

**Acceptance:**

- Versioned (`protocolVersion: "1.0"`), one JSON object per line.
- Every request has `id`; every response echoes it.
- A response carries `ok`, `result`, and `error`; a partial result is an error.
- Unknown fields are rejected at the top level.
- Valid, malformed, timeout, crash, and partial-result cases are documented.

**Read:** [`../docs/01-architecture.md`](../docs/01-architecture.md) §6.

**Evidence:** `schemas/protocol.schema.json`; protocol fixtures.

### 0.3 — Write the `GraphSnapshot` schema

**Task:** Model files, symbols, modules, entries, edges, certainty, SCCs, live
and candidate bitsets, hashes, and versions.

**Acceptance:**

- Every ID is a stable string key in JSON (for example
  `src/lib/format.ts#formatInvoice`).
- Certainty is an enum: `certain`, `probable`, `unknown`.
- An edge has source, target, kind, and certainty.
- The schema rejects a snapshot without `configHash`, `resolverHash`,
  `toolVersion`, or `protocolVersion`.

**Read:** [`../docs/03-algorithms.md`](../docs/03-algorithms.md) §4 and §12.

**Evidence:** `schemas/graph-snapshot.schema.json`.

### 0.4 — Write the `Delta` schema

**Task:** Model newly dead symbols and files, lost witnesses, triggers,
confidence, cascade summary, batches summary, and incomparability.

**Acceptance:**

- Every delta item has a witness or an explicit `no-witness` reason.
- The trigger is a changed edge or `indirect`, never an intent claim.
- An incomparable pair has a named reason.
- A no-op delta validates.

**Read:** [`../docs/04-whole-graph-delta.md`](../docs/04-whole-graph-delta.md) §7.

**Evidence:** `schemas/delta.schema.json`; `delta-no-change` fixture.

### 0.5 — Write the `DeletionPlan` schema

**Task:** Model batches, symbols, files, patches, impact, blast radius, and
policy.

**Acceptance:**

- A batch has an id, symbols, files, and expected effect.
- No batch may share a file with another batch (validated by a script).
- `blastRadius` records the policy and the decision (`allow`, `split`,
  `refuse`, `override`).
- An empty plan validates.

**Read:** [`../docs/03-algorithms.md`](../docs/03-algorithms.md) §18–§19.

**Evidence:** `schemas/deletion-plan.schema.json`.

### 0.6 — Write the `Receipt` schema

**Task:** Model tool, base, head, plan, build, batch, checks, baseline status,
outcome, rejected, policy, environment, and content hash.

**Acceptance:**

- `outcome` is one of `verified`, `rejected`, `known-failure`, `flaky`, `error`,
  `revert-failed`, `interaction-failure`.
- A `verified` receipt has at least one check and no rejected set.
- A check records command, status, baseline status, duration, and reruns.
- The schema requires `contentHash`.

**Read:** [`../docs/05-verified-deletion.md`](../docs/05-verified-deletion.md) §4.

**Evidence:** `schemas/receipt.schema.json`.

### 0.7 — Write the `Config` schema

**Task:** Model `shrike.config.ts` after evaluation: include/ignore, tsconfig,
entry policy, checks, batch policy, build evidence, receipts, and output.

**Acceptance:**

- Every field has a default documented.
- Unknown keys are rejected with a helpful error.
- A minimal config (`{}`) validates and produces documented defaults.
- The schema is versioned.

**Read:** [`../docs/01-architecture.md`](../docs/01-architecture.md) §6 and
[`../docs/05-verified-deletion.md`](../docs/05-verified-deletion.md) §2.3.

**Evidence:** `schemas/config.schema.json`.

### 0.8 — Scaffold the Rust workspace

**Task:** Create `shrike-core`, `shrike-protocol`, and `shrike-engine` with
minimal code and tests.

**Acceptance:**

- `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test` pass.
- `shrike-engine --version` prints the tool version and protocol version.
- `#![forbid(unsafe_code)]` in every crate.
- Edition 2024, pinned toolchain.

**Read:** [`../docs/02-folder-structure.md`](../docs/02-folder-structure.md) §3.

**Evidence:** CI log.

### 0.9 — Scaffold TypeScript packages

**Task:** Create `packages/cli` and `packages/testkit` with build, lint, and
test scripts.

**Acceptance:**

- `pnpm lint`, `pnpm typecheck`, and `pnpm test` pass.
- The CLI prints usage and exits 2 on an unknown command.
- No package imports another package's `src/` path.
- ESM, Node 22+.

**Read:** [`../docs/02-folder-structure.md`](../docs/02-folder-structure.md) §4.

**Evidence:** CI log.

### 0.10 — Build the fixture harness

**Task:** Implement `packages/testkit` helpers: create a temp repo, copy a
fixture, run a command, compare JSON, and clean up.

**Acceptance:**

- A fixture can be checked out at a temp path.
- The harness never mutates the source fixture.
- Failures print a readable diff.
- A fixture with a missing expected file fails clearly.

**Read:** [`../docs/02-folder-structure.md`](../docs/02-folder-structure.md) §6.

**Evidence:** harness tests.

### 0.11 — Build the must-not-report corpus

**Task:** Add ≥ 100 entries across the categories in
[`../docs/08-benchmarks-and-confidence.md`](../docs/08-benchmarks-and-confidence.md) §2.

**Acceptance:**

- Each entry has a README with a framework citation.
- Each entry has `expected.json` with `"findings": []` or a blocked reason.
- The corpus runner fails on a single finding.
- The corpus is independent of the analyzer's output.

**Evidence:** corpus report.

### 0.12 — Build delta fixtures

**Task:** Add the nine scenarios from
[`../docs/04-whole-graph-delta.md`](../docs/04-whole-graph-delta.md) §11.

**Acceptance:**

- Each fixture has a base tree, a head tree, and `expected.json`.
- `delta-no-change` produces an empty delta.
- `delta-removed-caller` includes the untouched file.
- `delta-cycle` never yields a partial SCC.

**Evidence:** fixture suite.

### 0.13 — Build verification fixtures

**Task:** Add pass, fail, flaky, known-failure, timeout, and revert-failure
fixtures.

**Acceptance:**

- Each fixture has a check command with a deterministic result.
- The flaky fixture is deterministic across reruns (a fixed sequence).
- The revert-failure fixture is isolated and never runs in the default suite.

**Read:** [`../docs/05-verified-deletion.md`](../docs/05-verified-deletion.md) §8.

**Evidence:** fixture suite.

### 0.14 — Wire CI

**Task:** Add `.github/workflows/ci.yml` and `docs.yml`.

**Acceptance:**

- Rust: fmt, clippy, test.
- Node: lint, typecheck, test.
- Schemas validate.
- Fixtures and corpus run.
- Plan/tracker IDs match.
- Relative Markdown links resolve.

**Evidence:** CI log.

### 0.15 — Record decisions and tracker

**Task:** Record D-001…D-016 (and D-017/D-018 if accepted) and confirm the phase
tables match the plans.

**Acceptance:**

- Every task ID in this plan exists in [`../TRACKER.md`](../TRACKER.md) §4.
- Every decision has an entry in [`../docs/12-decision-log.md`](../docs/12-decision-log.md).
- The open questions table is current.

**Evidence:** tracker diff.

---

## 5. Exit gate

All of the following must be demonstrated:

- [ ] `cargo test` and `cargo clippy` are green.
- [ ] `pnpm test`, `pnpm typecheck`, and `pnpm lint` are green.
- [ ] All six schemas validate and have at least one valid example.
- [ ] The must-not-report corpus has ≥ 100 entries and runs.
- [ ] The nine delta fixtures exist and are registered.
- [ ] The engine responds to `version` over the protocol.
- [ ] `scripts/check-links.mjs` passes.
- [ ] `scripts/check-docs.mjs` passes.
- [ ] The tracker records the evidence for 0.1–0.15.

---

## 6. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Schemas drift from Rust types | Generate schemas from Rust where practical; validate Rust output against schemas in CI |
| The corpus encodes rules | Build it from framework docs and real apps before rules |
| The protocol is over-designed | Five commands only; additive fields allowed only within a major version |
| Fixtures are too synthetic | Include one real-app snapshot pinned by commit |
| CI is slow from day one | Split Rust and Node jobs; cache dependencies |

---

## 7. Definition of done

PLAN-0 is done when a new contributor can clone the repository, run one command,
and see the contract, schemas, fixtures, and CI pass — with no parser, no
resolver, and no product claims. That is the foundation PLAN-1 builds on.
