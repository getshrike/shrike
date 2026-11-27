# Architecture and change map

## Two runtimes, one contract

```text
schemas ──► crates (Rust engine) ──► engine binary
   │                                     │
   └────────► packages (TypeScript host) ◄┘
                      │
                      ▼
        fixtures / benchmarks / scripts / skills / .github
```

- `schemas/` is the contract: seven JSON Schema 2020-12 files for the protocol,
  graph snapshot, delta, deletion plan, receipt, config, and cleanup report.
  Changing one is a protocol change; update both runtimes in the same PR.
- Rust crates never depend on TypeScript packages. The host talks to the engine
  over newline-delimited JSON, never by linking it.
- Fixtures and benchmarks are consumers of the engine/host, not libraries.

## Pipeline

```text
project + config
  → parse (oxc, Rust) → semantic summaries → module resolution
  → graph + framework entries → reachability with witnesses
  → base snapshot + head snapshot → orphan delta
  → SCC condensation + cascade closure → bounded deletion batches
  → apply batch in a worktree → baseline checks → after checks → compare
  → pass: keep + receipt / fail: bisect, revert culprit, retry remainder
  → draft PR + portable receipt
```

Each arrow is a phase with its own invariants; the full phase-by-phase algorithm,
data model, and a worked example are in [how-it-works.md](how-it-works.md).

## Rust crates (`crates/`)

| Crate | Responsibility |
| --- | --- |
| `shrike-core` | IDs, spans, diagnostics, hashing, config; no other Shrike dependency |
| `shrike-protocol` | Request/response types and JSON codec |
| `shrike-syntax` | Parsing and file summaries (oxc); framework/dynamic import collection |
| `shrike-resolve` | Path, tsconfig, workspace, and barrel resolution |
| `shrike-graph` | Graph build, framework entries, reachability, liveness, diagnostics |
| `shrike-delta` | Base/head comparison, changed-edge attribution, cascade closure |
| `shrike-plan` | Conflict detection, batch partition, blast radius, impact |
| `shrike-evidence` | Optional build-evidence join (routes, bytes, verdicts, fallbacks) |
| `shrike-engine` | Binary: dispatch, snapshot cache, stdout JSON |

Actual crate dependencies: `protocol → core`, `syntax → core`, `resolve → core`,
`graph → {core, syntax, resolve}`, `delta → {core, graph}`, `plan → {core, graph,
delta}`, `evidence → {core, graph, syntax}`, `engine → all runtime crates`.
`shrike-graph` and `shrike-resolve` read manifests/configs (package.json,
next.config, cypress config, ...) during analysis; the engine owns the snapshot
cache and protocol output. No crate spawns project processes or opens sockets.

## TypeScript packages (`packages/`)

| Package | Responsibility |
| --- | --- |
| `cli` | Public command surface, config loading, engine spawn, output, exit codes |
| `verifier` | Baselines, worktrees, apply, check execution, compare, bisect, revert |
| `receipts` | Write, validate, hash, and render receipts; does not run checks |
| `agent` | Hooks, MCP server, agent brief; calls the CLI, never analyzes |
| `action` | Thin GitHub Action wrapper; no business logic |
| `testkit` | Dev-only fixture/assertion helpers; never published |
| `protocol` | Private, shared TypeScript types for the wire protocol and JSON artifacts; mirrors `schemas/` |
| `platform/*` | Metadata and staged native binaries for each target |

ESM, Node 22+, strict TypeScript. The published CLI is a bundle with no runtime
`@shrike/*` dependencies: `scripts/build-cli.mjs` inlines `verifier`, `receipts`,
`agent`, and `testkit` from workspace devDependencies; the Action bundle is
built separately by `scripts/build-action.mjs`. The same build copies the
canonical public skill from `skills/` to `packages/cli/dist/skills/`; the
installer loads that copy and fails closed when it is missing. The CLI also
ships a lazily loaded `dist/tui.js` for the interactive report;
`scripts/build-cli.mjs` produces it and `check-package.mjs` checks it. Keep
those packages free of undeclared cross-imports, and regenerate the bundles
after changes.

## Where to make a change

| Change | Start here |
| --- | --- |
| New framework entry or protected export | `shrike-graph/src/builder/entries/` (`detect.rs` per-file rules, `precompute.rs` package facts) |
| Framework config parsing / shared config helper | `shrike-graph/src/config/` (`mod.rs` holds the shared helpers) |
| Wire-protocol or artifact type (TS) | `packages/protocol/src/index.ts` and the matching schema in `schemas/` |
| Parser or dynamic-import behavior | `shrike-syntax/src/parse/` |
| Resolution rule | `shrike-resolve/src/` |
| Delta attribution or cascade | `shrike-delta/src/` |
| New diagnostic category | parser summary → graph builder → `cli/src/diagnostics.ts` |
| New CLI flag or output | `packages/cli/src/` (+ schema if JSON shape changes) |
| Verifier check/baseline behavior | `packages/verifier/src/` |
| Receipt field | `schemas/receipt.schema.json`, `packages/receipts` |
| New fixture or gate | [testing.md](testing.md) |

A change that alters analysis output must bump `ANALYSIS_VERSION` in
`crates/shrike-engine/src/cache.rs` so snapshots invalidate. A schema change
must update the schema, both runtimes, and fixtures in the same PR.

## Verification and receipts

The verifier stores baseline → apply → check → compare → revert-or-keep in the
receipt. Worktree mode is the default; `--in-place` is explicit. Checks are
typecheck/lint/test/build tiers read from project scripts or `shrike.config.ts`.
Comparison is against the baseline: known failures do not block, new failures,
timeouts, and regressions do. A failed batch is bisected and reverted; a failed
revert exits 5 with a recovery snapshot.

Never weaken this contract to make a deletion pass. Build evidence can add
liveness but can never authorize a deletion.
