# 02 — Technical Substrate

> **Parent:** [README.md](README.md) · **Status:** research note · **Date:** 2026-11-20 · **Confidence:** medium
> **Method:** crate docs, repository review, design constraints.

---

## 1. Question

Which parser, semantic model, resolver, graph, and runtime substrate should the
Rust engine use, and what are the known limits?

---

## 2. Parser and semantic model

### 2.1 `oxc_parser`

- Fast Rust parser for JavaScript and TypeScript.
- Produces an AST with spans, comments, and diagnostics.
- Supports JSX, TSX, decorators, and modern syntax.
- The AST is large; do not retain it after the per-file summary.

**Decision:** use `oxc_parser`; do not build a second parser.

### 2.2 `oxc_semantic`

- Builds scopes, symbols, and references over the AST.
- Distinguishes declarations from references.
- Exposes unresolved references.
- Suitable for binding imports to local names and for detecting unused locals.

**What it gives Shrike:**

- a symbol table per file;
- reference edges within a file;
- unresolved names (globals, ambient, dynamic);
- scope-aware shadowing.

**What it does not give Shrike:**

- cross-file resolution (Shrike builds that);
- type information (the project's typecheck is the authority);
- framework conventions (adapters);
- runtime behavior.

**Risk:** API churn. Pin the version; isolate it behind `shrike-syntax`; upgrade
behind the fixture suite.

### 2.3 TypeScript 7 semantic API

TypeScript 7 exposes a semantic API as a stdio server, not an in-process JS API.
A TypeScript host does not get in-process type checking for free.

**Implication:** do not design the engine around an in-process TypeScript
checker. Run the project's `tsc` as a check in the verifier. Use `oxc` for the
graph.

---

## 3. Resolution

### 3.1 What must be resolved

- relative specifiers (`./`, `../`);
- tsconfig `paths` and `baseUrl`;
- workspace package names;
- package `exports`, `main`, `module`, `types`;
- barrel re-exports;
- framework aliases (`@/`, `~`, custom);
- extension and `index` resolution.

### 3.2 Why resolution is the hard part

A wrong resolution creates a wrong edge. A missing edge creates a false positive.
A false positive is a release blocker.

Rules:

- unresolved is recorded, not guessed;
- a barrel re-export creates an edge to the underlying symbol;
- conditional exports are evaluated in the documented order;
- a wildcard export is matched deterministically;
- a package outside the workspace is external and is not traversed.

### 3.3 Monorepos

- Detect pnpm, npm, and yarn workspaces.
- Build a package map from `package.json` names to entry files.
- A workspace package's public entry is an entry; internals are not.
- Cross-package deletion requires an explicit allow.
- A shared package used by two apps keeps its exports live.

---

## 4. Graph representation

| Structure | Choice | Why |
|---|---|---|
| Symbol arena | `Vec<SymbolRecord>` | Cache-friendly, deterministic |
| Outgoing edges | CSR (`offsets`, `edges`) | Fast BFS, compact |
| Incoming edges | CSR built once | Closure and referrer checks |
| Live set | `FixedBitSet` | O(1) membership, compact |
| SCC | `Vec<SccId>` + members | Tarjan output, deterministic |
| Condensation | CSR over `SccId` | Closure on the DAG |
| Strings | Interner | Memory and deterministic keys |

Avoid `HashMap` iteration in output. Use a sorted `Vec` or a `BTreeMap` for
serialization.

---

## 5. Algorithms and crates

| Need | Option | Notes |
|---|---|---|
| SCC | hand-written iterative Tarjan | Avoid recursion; deterministic |
| Graph utilities | `petgraph` (optional) | Useful for tests, not necessarily the hot path |
| Parallelism | `rayon` | Per-file parse; deterministic merge |
| Hashing | `blake3` | Fast, content-addressed |
| Hashing (non-crypto) | `rustc-hash` | Internal maps only |
| Serialization | `serde` + `serde_json` | Explicit field names |
| Errors | `thiserror` (libs), `anyhow` (binary/xtask) | Typed errors in libraries |
| Logging | `tracing` | Structured, no `println!` in libs |
| Snapshot tests | `insta` | Parse and graph output |
| Property tests | `proptest` | ID stability, SCC invariants |
| Benchmarks | `criterion` + corpus harness | Micro and end-to-end |

**Rule:** a crate is a dependency only if it removes more risk than it adds. The
graph core should be small and auditable.

---

## 6. Cache

| Layer | Key | Value |
|---|---|---|
| Parse | file hash + parser version | summary, spans, directives, diagnostics |
| Resolve | importer + specifier + resolver hash | resolved module ID |
| Snapshot | file hashes + config + resolver + tool + protocol | serialized snapshot |
| Build | snapshot hash + Next version | joined module map |
| Plan | base/head snapshot hashes + policy | plan |

- Content-addressed, atomic writes, safe to delete.
- A corrupt entry is discarded and recorded.
- No source text beyond names and spans.
- Cache version bumps with the parser or protocol.

---

## 7. Determinism

Sources of nondeterminism to eliminate:

- hash-map iteration order;
- parallel completion order;
- filesystem directory order;
- pointer addresses in IDs;
- timestamps or random IDs in output;
- floating-point formatting;
- locale-dependent sorting.

Techniques:

- sort files by normalized path before parse;
- merge parallel results in file order;
- use typed arenas and integer IDs;
- sort edges by `(source, target, kind, certainty)`;
- serialize with explicit field order;
- test at 1, 2, and 8 threads.

---

## 8. Performance envelope

| Workload | Target |
|---|---|
| 1k cold | ≤ 1.5 s |
| 1k warm | ≤ 400 ms |
| 10k cold | ≤ 6 s |
| 10k warm | ≤ 2 s |
| 10k peak RSS | ≤ 600 MB |

Levers:

- per-file summaries, not ASTs;
- interning;
- CSR;
- `rayon` with deterministic merge;
- bounded channels for parse results;
- no whole-repo string copies;
- cache warm path.

**Rule:** profile before optimizing. Record the measurement.

---

## 9. Known limits

- No type information in the graph; type-only edges are approximate.
- Dynamic access cannot be fully resolved; unknown edges block.
- A generated file may be unparseable or meaningless; policy excludes it.
- A monorepo with multiple Next.js apps needs per-app build evidence (PLAN-6).
- Native binary distribution adds a platform matrix.
- `oxc` API churn is a maintenance cost.

---

## 10. Decision impact

- Use `oxc` for parse and semantics.
- Build a deterministic resolver in Rust.
- Use CSR and bitsets.
- Keep the engine separate from the host.
- Pin dependencies and isolate churn.

---

## 11. Sources

See [`06-sources.md`](06-sources.md).
