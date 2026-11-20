# 02 — Folder Structure

> **Parent:** [MASTER_PROMPT.md](../MASTER_PROMPT.md) · **Tracker:** [TRACKER.md](../TRACKER.md)
> **Status:** canonical · Authored 2026-11-20

---

## 1. Why the structure matters

Shrike has two runtimes, two dependency ecosystems, and three kinds of artifacts
that must agree:

- the Rust engine that computes the graph;
- the TypeScript host that executes the config and the project's checks;
- the schemas, fixtures, and receipts that define the contract between them.

A flat `src/` would mix them. A single language would force the wrong runtime.
The repository is therefore a **polyglot monorepo with strict dependency
direction**:

```text
schemas  ──►  crates  ──►  engine binary
   │                         │
   └────────►  packages  ◄───┘
                  │
                  ▼
              fixtures / benchmarks / scripts / docs
```

- **Schemas are the source of truth.** Both runtimes generate or validate
  against them.
- **Crates never depend on packages.** The engine is a standalone binary.
- **Packages depend on the engine through the protocol**, never by linking it.
- **Fixtures and benchmarks are consumers**, not libraries.
- **Docs, plans, prompts, and research live at the root**, because they are the
  product definition and must be readable without entering a package.

### 1.1 Prior art and conventions

The layout follows established conventions for multi-crate Rust workspaces and
polyglot monorepos:

- The [Cargo Book](https://doc.rust-lang.org/cargo/reference/workspaces.html)
  recommends keeping workspace members in a flat directory, "commonly
  `crates/`", and using a glob for the `members` field.
- The [Microsoft Pragmatic Rust Guidelines](https://microsoft.github.io/rust-guidelines/guidelines/project)
  require all crates to be siblings in one folder and treat crates nested under
  another crate's `src/` as unacceptable.
- [matklad's *Large Rust Workspaces*](https://matklad.github.io/2021/08/22/large-rust-workspaces.html)
  recommends the flat `crates/` layout for workspaces between 10,000 and one
  million lines, and warns against stripping a common prefix from directory
  names.
- [Tauri](https://github.com/tauri-apps/tauri) and
  [SWC](https://github.com/swc-project/swc) use the same `crates/` + `packages/`
  split for their Rust and TypeScript halves.

A top-level `src/` is deliberately not used: in Cargo, `src/` belongs to a
package, and the workspace root is a virtual manifest with no package of its
own. Every crate already has its own `src/`.

---

## 2. Top-level layout

```text
.
├── MASTER_PROMPT.md
├── README.md
├── TRACKER.md
├── docs/
├── plans/
├── prompts/
├── research/
├── schemas/
├── fixtures/
├── benchmarks/
├── crates/
├── packages/
├── scripts/
├── .github/
│   └── workflows/
├── Cargo.toml
├── Cargo.lock
├── package.json
├── pnpm-workspace.yaml
├── tsconfig.base.json
├── rust-toolchain.toml
├── .editorconfig
├── .gitignore
├── LICENSE
└── CHANGELOG.md
```

### 2.1 Root files

| File | Purpose |
|---|---|
| `MASTER_PROMPT.md` | Canonical product and engineering brief |
| `README.md` | Public entry point and repository map |
| `TRACKER.md` | Status, tasks, decisions, metrics, evidence |
| `Cargo.toml` | Rust workspace manifest |
| `Cargo.lock` | Locked Rust dependencies; committed for binaries |
| `package.json` | Root scripts and dev dependencies; private |
| `pnpm-workspace.yaml` | TypeScript workspace definition |
| `tsconfig.base.json` | Shared TypeScript configuration |
| `rust-toolchain.toml` | Pinned Rust toolchain |
| `.gitignore` | Ignores build outputs, caches, and local receipts |
| `LICENSE` | Decided in PLAN-5 |
| `CHANGELOG.md` | User-visible changes; generated from changesets |

---

## 3. `crates/` — Rust engine

```text
crates/
├── shrike-core/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── ids.rs
│       ├── intern.rs
│       ├── span.rs
│       ├── diagnostic.rs
│       ├── hash.rs
│       └── error.rs
├── shrike-protocol/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── request.rs
│       ├── response.rs
│       ├── codec.rs
│       └── version.rs
├── shrike-syntax/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── parse.rs
│       ├── summary.rs
│       ├── imports.rs
│       ├── exports.rs
│       ├── jsx.rs
│       ├── directives.rs
│       └── dynamic.rs
├── shrike-resolve/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── relative.rs
│       ├── tsconfig.rs
│       ├── workspace.rs
│       ├── package.rs
│       └── barrel.rs
├── shrike-graph/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── snapshot.rs
│       ├── module_graph.rs
│       ├── symbol_graph.rs
│       ├── entries.rs
│       ├── reachability.rs
│       ├── witness.rs
│       ├── scc.rs
│       └── proof.rs
├── shrike-delta/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── compare.rs
│       ├── orphan.rs
│       ├── trigger.rs
│       └── cascade.rs
├── shrike-plan/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── conflict.rs
│       ├── partition.rs
│       ├── blast_radius.rs
│       ├── impact.rs
│       └── bisect.rs
├── shrike-evidence/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── next.rs
│       ├── join.rs
│       ├── bytes.rs
│       └── status.rs
├── shrike-receipt/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── model.rs
│       ├── hash.rs
│       └── sign.rs
├── shrike-engine/
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── dispatch.rs
│       ├── output.rs
│       └── error.rs
└── xtask/
    ├── Cargo.toml
    └── src/
        ├── main.rs
        ├── fixtures.rs
        ├── schema.rs
        ├── bench.rs
        └── release.rs
```

### 3.1 Crate rules

- `shrike-core` has no dependency on any other Shrike crate.
- `shrike-protocol` depends only on `shrike-core`.
- `shrike-syntax` depends on `shrike-core` and `oxc_*`.
- `shrike-resolve` depends on `shrike-core` and `shrike-syntax`.
- `shrike-graph` depends on `shrike-syntax`, `shrike-resolve`, and `shrike-core`.
- `shrike-delta` depends on `shrike-graph`.
- `shrike-plan` depends on `shrike-delta`.
- `shrike-evidence` depends on `shrike-graph`.
- `shrike-receipt` depends on `shrike-core`.
- `shrike-engine` depends on all runtime crates.
- `xtask` may depend on anything; it is dev-only.
- Each crate directory name equals its package name: `crates/shrike-graph`
  contains `name = "shrike-graph"`. Do not strip the `shrike-` prefix; directory
  and package names stay in sync. `xtask` is the exception because it is a
  conventional tooling crate, not a Shrike runtime crate.
- The workspace uses `members = ["crates/*"]` and lists the runtime crates in
  `default-members`, so a root `cargo build` or `cargo test` skips `xtask`; CI
  still passes `--workspace`.
- No crate depends on a TypeScript package.
- No crate performs I/O outside its documented boundary:
  - `shrike-syntax` reads source files;
  - `shrike-engine` reads/writes the cache and stdout;
  - everything else is pure data.

### 3.2 Rust conventions

- Edition 2024, pinned toolchain.
- `#![forbid(unsafe_code)]` in every crate unless a measured need is documented.
- `thiserror` for library errors; `anyhow` only in `shrike-engine` and `xtask`.
- `tracing` for structured logs; no `println!` outside the engine's output path.
- `serde` with explicit field names; no default serialization that can drift.
- `insta` for snapshot tests.
- `proptest` for invariant tests (ID stability, witness validity, no partial SCC).
- `criterion` for microbenchmarks; the corpus harness for end-to-end.

---

## 4. `packages/` — TypeScript host

```text
packages/
├── cli/
│   ├── package.json
│   ├── tsconfig.json
│   └── src/
│       ├── index.ts
│       ├── bin.ts
│       ├── commands/
│       │   ├── scan.ts
│       │   ├── delta.ts
│       │   ├── plan.ts
│       │   ├── proof.ts
│       │   ├── prune.ts
│       │   ├── verify.ts
│       │   ├── receipt.ts
│       │   ├── evidence.ts
│       │   ├── cache.ts
│       │   └── hook.ts
│       ├── config/
│       │   ├── load.ts
│       │   ├── schema.ts
│       │   └── defaults.ts
│       ├── engine/
│       │   ├── locate.ts
│       │   ├── spawn.ts
│       │   └── protocol.ts
│       ├── output/
│       │   ├── human.ts
│       │   ├── json.ts
│       │   └── exit.ts
│       └── errors.ts
├── verifier/
│   ├── package.json
│   └── src/
│       ├── baseline.ts
│       ├── checks.ts
│       ├── worktree.ts
│       ├── apply.ts
│       ├── compare.ts
│       ├── signature.ts
│       ├── flaky.ts
│       ├── bisect.ts
│       ├── revert.ts
│       └── index.ts
├── receipts/
│   ├── package.json
│   └── src/
│       ├── write.ts
│       ├── validate.ts
│       ├── hash.ts
│       ├── render.ts
│       └── index.ts
├── agent/
│   ├── package.json
│   └── src/
│       ├── hook/
│       │   ├── install.ts
│       │   ├── claude.ts
│       │   ├── cursor.ts
│       │   └── opencode.ts
│       ├── brief.ts
│       ├── bundle.ts
│       ├── mcp.ts
│       └── index.ts
├── action/
│   ├── package.json
│   ├── action.yml
│   └── src/
│       └── main.ts
├── engine-linux-x64-gnu/
│   ├── package.json
│   └── bin/shrike-engine
├── engine-linux-arm64-gnu/
├── engine-darwin-arm64/
├── engine-darwin-x64/
├── engine-win32-x64-msvc/
└── testkit/
    ├── package.json
    └── src/
        ├── repo.ts
        ├── fixture.ts
        ├── snapshot.ts
        └── index.ts
```

### 4.1 Package rules

- `cli` owns the public command surface; no other package writes to stdout.
- `verifier` does not know about the CLI's argument parsing; it receives a
  validated plan and a config.
- `receipts` does not run checks; it records results.
- `agent` does not analyze; it calls the CLI or the engine.
- `action` is a thin wrapper; no business logic.
- `engine-*` packages contain only the binary and metadata.
- `testkit` is dev-only and never published.
- All packages are ESM, Node 22+.
- No package imports another package's `src/` path; use the package entry point.
- No circular dependencies. The dependency direction is:
  `cli → verifier → receipts → testkit` and `cli → agent`, `action → cli`.

### 4.2 TypeScript conventions

- `strict: true`, `noUncheckedIndexedAccess: true`.
- `zod` for runtime validation of config and protocol messages.
- `execa` or `node:child_process` with explicit timeouts for checks.
- `picocolors` for terminal output; no heavy CLI framework.
- `vitest` for unit and integration tests.
- `tsup` or `tsc` for builds; no bundler magic.
- No `any` in public APIs. Use `unknown` + schema validation.
- Every command returns an exit code; no `process.exit` scattered through
  libraries.

---

## 5. `schemas/` — the contract

```text
schemas/
├── protocol.schema.json
├── graph-snapshot.schema.json
├── delta.schema.json
├── deletion-plan.schema.json
├── receipt.schema.json
└── config.schema.json
```

Rules:

- JSON Schema 2020-12.
- Every schema has `$id`, `title`, `description`, and `additionalProperties:
  false` at the top level.
- Field names are `camelCase` in JSON, `snake_case` in Rust, mapped explicitly
  with serde rename attributes.
- A schema change is a protocol change; bump the version and update both
  runtimes in the same PR.
- CI validates every example in the docs and every fixture output.
- Schemas are generated from Rust types where practical; hand-written schemas
  must have a test that they accept the Rust output.

---

## 6. `fixtures/` — the oracle

```text
fixtures/
├── next-app-router/
├── next-pages-router/
├── react-spa/
├── monorepo/
├── package-public-api/
├── dynamic-edges/
├── delta/
│   ├── removed-caller/
│   ├── removed-route/
│   ├── renamed-export/
│   ├── barrel/
│   ├── cycle/
│   ├── dynamic/
│   ├── monorepo/
│   ├── no-change/
│   └── preexisting/
├── verification/
│   ├── pass/
│   ├── fail/
│   ├── flaky/
│   ├── known-failure/
│   ├── timeout/
│   └── revert-failure/
├── build-graph/
│   ├── next-16.4/
│   ├── next-16.1/
│   ├── next-15/
│   └── missing/
└── must-not-report/
    ├── next-routes/
    ├── server-actions/
    ├── client-boundaries/
    ├── metadata/
    ├── proxy/
    ├── dynamic-imports/
    ├── tests/
    └── packages/
```

Rules:

- Every fixture has a `README.md` explaining what it proves.
- Every fixture has an `expected.json` with the exact delta/plan/receipt.
- Fixtures are small enough to review by hand, except the real-app snapshots,
  which are pinned by commit and stored as generated output.
- A fixture is added before a fix, not after.
- The must-not-report corpus is generated from framework documentation and real
  applications, not from the rules' output.

---

## 7. `benchmarks/` — the evidence

```text
benchmarks/
├── corpus/
│   ├── next-app-1k.json
│   ├── next-app-3k.json
│   ├── monorepo-10k.json
│   └── library-5k.json
├── harness/
│   ├── run.ts
│   ├── measure.ts
│   └── report.ts
└── reports/
    ├── 2026-11-20-reference.json
    └── README.md
```

Rules:

- Pin the commit, Node version, Rust version, CPU, RAM, and OS.
- Run 3 warmups and 5 measured runs.
- Record cold and warm separately.
- Record peak RSS, not just time.
- Store raw JSON, not just a summary.
- Every public benchmark links to a raw report.
- A benchmark that cannot be reproduced is removed.

---

## 8. `scripts/` — automation

```text
scripts/
├── validate-schemas.mjs
├── corpus.mjs
├── delta-fixtures.mjs
├── verification-fixtures.mjs
├── bench.mjs
├── check-links.mjs
├── check-docs.mjs
├── release.mjs
└── package-binary.mjs
```

Rules:

- Scripts are Node ESM, no external runtime beyond Node and pnpm.
- Every script is idempotent and safe to run locally.
- `check-links.mjs` validates every relative Markdown link in the repository.
- `check-docs.mjs` validates that every plan task ID exists in the tracker and
  vice versa.
- `release.mjs` is the only script allowed to publish.

---

## 9. `.github/workflows/`

```text
.github/workflows/
├── ci.yml
├── corpus.yml
├── bench.yml
├── release.yml
└── docs.yml
```

- `ci.yml`: Rust fmt/clippy/test, Node lint/typecheck/test, schema validation,
  fixture suite.
- `corpus.yml`: must-not-report corpus on every PR and nightly on the labeled
  set.
- `bench.yml`: performance regression on `main`, not on every PR.
- `release.yml`: tag-driven binaries, checksums, npm publish, provenance.
- `docs.yml`: link check and plan/tracker consistency.

---

## 10. Dependency direction and boundaries

```text
schemas ──► crates ──► engine binary
   │                      │
   └──────► packages ◄────┘
               │
               ▼
        fixtures / benchmarks / scripts
```

Forbidden:

- a crate importing a TypeScript package;
- a package linking the Rust engine as a native addon;
- a fixture importing from a package's internals;
- a script duplicating logic that belongs in a crate or package;
- a doc linking to a file that does not exist;
- a plan task ID that does not match the tracker.

Allowed exceptions are recorded in the PR with a reason and a removal plan.

---

## 11. Naming

| Kind | Convention | Example |
|---|---|---|
| Rust crate | `shrike-<noun>` | `shrike-graph` |
| Rust module | `snake_case` | `module_graph.rs` |
| TypeScript package | `@shrike/<noun>` or `shrike` | `@shrike/verifier` |
| TypeScript file | `kebab-case` | `blast-radius.ts` |
| JSON schema | `kebab-case.schema.json` | `deletion-plan.schema.json` |
| Fixture | `kebab-case` | `removed-caller/` |
| Plan | `PLAN-<n>-<slug>.md` | `PLAN-2-graph-and-delta.md` |
| Prompt | `<nn>-<role>.md` | `01-rust-engine-agent.md` |
| Research | `<nn>-<topic>.md` | `01-competitive-landscape.md` |

---

## 12. What lives where: a decision table

| Artifact | Location | Why |
|---|---|---|
| Product thesis | `docs/00` | Read before code |
| Architecture | `docs/01` | Read before code |
| Algorithm | `docs/03` | Single source of truth |
| JSON contract | `schemas/` + `crates/shrike-protocol` | Both runtimes validate |
| Parser | `crates/shrike-syntax` | Hot path |
| Resolver | `crates/shrike-resolve` | Hot path |
| Graph | `crates/shrike-graph` | Hot path |
| Delta | `crates/shrike-delta` | Hot path |
| Batch plan | `crates/shrike-plan` | Hot path |
| Build evidence | `crates/shrike-evidence` | Hot path, optional |
| Receipt model | `crates/shrike-receipt` + `packages/receipts` | Rust computes, TS renders |
| CLI | `packages/cli` | User surface |
| Verifier | `packages/verifier` | Process orchestration |
| Hook | `packages/agent` | Distribution |
| Action | `packages/action` | CI |
| Tests | Colocated + `fixtures/` | Unit near code, integration in fixtures |
| Benchmarks | `benchmarks/` | Reproducible evidence |
| Release | `scripts/release.mjs` + `.github/workflows/release.yml` | One path |

---

## 13. Acceptance criteria

- [ ] `cargo metadata` shows the workspace with the crates and dependency
      direction above.
- [ ] `pnpm -r list` shows the packages and no cycles.
- [ ] Every schema has a validation test and a doc example.
- [ ] Every fixture has a README and an expected output.
- [ ] `scripts/check-links.mjs` passes.
- [ ] `scripts/check-docs.mjs` passes.
- [ ] CI runs the Rust, Node, schema, fixture, and corpus checks.
- [ ] The engine binary package resolves on the four primary platforms.
- [ ] No crate imports a TypeScript package; no package links Rust.

---

## 14. Open questions

| ID | Question | Owner |
|---|---|---|
| Q-024 | Which platforms are supported at first? | PLAN-5 |
| Q-031 | One npm package with optional deps, or a scope per platform? | PLAN-5 |
| Q-032 | Is the engine binary statically linked on Linux? | PLAN-5 |
| Q-033 | Do we publish a standalone binary outside npm? | PLAN-5 |
| Q-034 | Is `xtask` the only place for release logic? | PLAN-0 |
