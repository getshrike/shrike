# PLAN-1 — Rust Engine

> **Parent:** [MASTER_PROMPT.md](../MASTER_PROMPT.md) · **Tracker:** [TRACKER.md](../TRACKER.md)
> **Status:** `planned` · **Depends on:** PLAN-0 · **Unblocks:** PLAN-2, PLAN-3
> **Owner prompt:** [`../prompts/01-rust-engine-agent.md`](../prompts/01-rust-engine-agent.md)

---

## 1. Goal

Build the analysis engine that everything else depends on: inventory, parse,
semantic summary, resolution, graph, entries, reachability with witnesses,
snapshots, cache, and the engine binary.

This plan is successful when the engine can answer, for a real TypeScript
repository, three questions with evidence:

1. What is the symbol/module graph?
2. Which symbols are live from framework entries, and what is the shortest
   witness?
3. Which symbols are candidates, and what unknown edges block deletion?

It does **not** implement the base/head delta (PLAN-2) or the verifier (PLAN-3).

---

## 2. Scope

### In scope

- `shrike-core`, `shrike-protocol`, `shrike-syntax`, `shrike-resolve`,
  `shrike-graph`, `shrike-engine`.
- A per-file semantic summary using `oxc_parser` and `oxc_semantic`.
- A deterministic resolver for relative, alias, workspace, package, and barrel
  imports.
- Next.js App Router, Next.js Pages Router, React, package, test, and config
  entries.
- Multi-source BFS with witness parent pointers.
- Dynamic-edge abstraction with bounded scopes.
- Snapshot serialization and a content-addressed cache.
- The `snapshot`, `proof`, `version`, and `evidence` engine commands.
- Determinism and performance harnesses.

### Out of scope

- Base/head delta, trigger attribution, cascade closure (PLAN-2).
- Batch partitioning (PLAN-2).
- Build-graph ingestion (PLAN-4).
- Verification, receipts, PRs (PLAN-3).

---

## 3. Crate map

| Crate | Owns | Key files |
|---|---|---|
| `shrike-core` | IDs, interners, spans, diagnostics, hashing, errors | `ids.rs`, `intern.rs`, `span.rs`, `hash.rs`, `error.rs` |
| `shrike-protocol` | Envelope, commands, codec, versions | `request.rs`, `response.rs`, `codec.rs`, `version.rs` |
| `shrike-syntax` | Parse, semantic summary, imports/exports, JSX, directives, dynamic forms | `parse.rs`, `summary.rs`, `imports.rs`, `exports.rs`, `jsx.rs`, `directives.rs`, `dynamic.rs` |
| `shrike-resolve` | Relative, tsconfig, workspace, package, barrel | `relative.rs`, `tsconfig.rs`, `workspace.rs`, `package.rs`, `barrel.rs` |
| `shrike-graph` | Snapshot, module graph, symbol graph, entries, reachability, witness, SCC, proof | `snapshot.rs`, `module_graph.rs`, `symbol_graph.rs`, `entries.rs`, `reachability.rs`, `witness.rs`, `scc.rs`, `proof.rs` |
| `shrike-engine` | Binary, dispatch, output, errors | `main.rs`, `dispatch.rs`, `output.rs` |

Dependency direction is defined in
[`../docs/02-folder-structure.md`](../docs/02-folder-structure.md) §3.

---

## 4. Work breakdown

### 1.1 — `shrike-core`: IDs, interners, spans, errors

**Task:** Implement typed arenas and interners for `FileId`, `SymbolId`,
`ModuleId`, `EntryId`, `EdgeId`, and `SccId`; spans with byte offset, line, and
column; diagnostics; and a library error type.

**Acceptance:**

- IDs are `u32` indices into typed arenas.
- The same input produces the same IDs across runs.
- Serialization is stable and documented.
- No `unsafe`.
- Unit tests cover interning, duplicate keys, and span calculation (including
  multi-byte UTF-8).

**Read:** [`../docs/03-algorithms.md`](../docs/03-algorithms.md) §4.1.

**Evidence:** unit tests; snapshot test of serialized IDs.

### 1.2 — `shrike-protocol`: envelope and codec

**Task:** Implement the line-delimited JSON request/response types, version
negotiation, and error mapping.

**Acceptance:**

- One request per line, one response per line.
- `protocolVersion` and `id` are required and echoed.
- Unknown command → structured error, exit 2.
- Malformed JSON → structured error, exit 4.
- A response with a missing `result` or `error` is a protocol error.
- Round-trip tests for valid, malformed, timeout, crash, and partial results.

**Read:** [`../docs/01-architecture.md`](../docs/01-architecture.md) §6.

**Evidence:** contract tests; schema validation.

### 1.3 — `shrike-syntax`: parse and per-file summary

**Task:** Parse each file with `oxc_parser`; extract imports, exports,
re-exports, declarations, JSX uses, directives, dynamic forms, and framework
exports. Drop the AST after the summary.

**Acceptance:**

- Imports capture source, imported names, local names, and type-only.
- Exports capture named, default, re-export, namespace, and type-only.
- JSX captures component names, member expressions, and namespaced names.
- `"use client"` and `"use server"` are captured per module and per function.
- Dynamic forms capture literal, template, and computed specifiers.
- A parse error yields a diagnostic and `parseStatus: error`; no panic.
- No AST is retained after the summary.
- Fixture tests cover `.ts`, `.tsx`, `.js`, `.jsx`, `.mjs`, `.cjs`, and `.d.ts`.

**Read:** [`../docs/03-algorithms.md`](../docs/03-algorithms.md) §6.

**Evidence:** `fixtures/dynamic-edges/`; parser snapshot tests.

### 1.4 — `shrike-syntax`: semantic scopes, symbols, references

**Task:** Use `oxc_semantic` to bind identifiers to declarations, distinguish
local from imported references, and record unresolved references.

**Acceptance:**

- Each declaration has a symbol with name, kind, span, and export status.
- Each reference points to a symbol or is recorded as unresolved.
- Shadowing is respected.
- A reference to an imported binding creates an edge to the imported symbol,
  not to the local alias.
- Unresolved references are recorded with the name and span; they are not
  dropped.
- No AST retention; only the summary and reference table survive.

**Read:** [`../docs/03-algorithms.md`](../docs/03-algorithms.md) §6.1.

**Evidence:** semantic fixture tests; unresolved-reference fixtures.

### 1.5 — `shrike-resolve`: resolver

**Task:** Resolve relative, tsconfig `paths`/`baseUrl`, workspace packages,
package `exports`/`main`/`module`/`types`, barrels, and framework aliases.

**Acceptance:**

- Resolution order follows
  [`../docs/03-algorithms.md`](../docs/03-algorithms.md) §7.
- Extension resolution covers `.ts`, `.tsx`, `.js`, `.jsx`, `.mjs`, `.cjs`,
  `.d.ts`, and `index.*`.
- `package.json` `exports` conditions (`import`, `require`, `types`, `default`)
  are respected.
- A barrel re-export creates an edge to the underlying symbol.
- An unresolved specifier is recorded and creates a diagnostic.
- Resolution is cached by `(importer, specifier, resolverConfigHash)`.
- Monorepo package names resolve through the workspace map.
- Fixture tests cover alias, workspace, conditional exports, and barrel.

**Read:** [`../docs/03-algorithms.md`](../docs/03-algorithms.md) §7.

**Evidence:** resolver fixtures; monorepo fixture.

### 1.6 — `shrike-graph`: snapshot and module graph

**Task:** Build the module graph, the symbol graph, CSR edge storage, and the
serializable snapshot.

**Acceptance:**

- Files, symbols, modules, entries, and edges are sorted deterministically.
- Outgoing and incoming edges are available.
- Every edge carries kind and certainty.
- The snapshot hash is BLAKE3 over canonical serialization.
- The snapshot contains no source text beyond names and spans.
- The snapshot deserializes without re-parsing.
- Invariant tests: every edge's source and target exist; no duplicate edge;
  every symbol belongs to one file.

**Read:** [`../docs/03-algorithms.md`](../docs/03-algorithms.md) §4.3 and §12.

**Evidence:** snapshot tests; invariant tests.

### 1.7 — Entry adapters: Next.js App Router

**Task:** Detect App Router entries and entry roles.

**Acceptance:**

- `page`, `layout`, `template`, `loading`, `error`, `global-error`,
  `not-found`, `route`, and `default` are entries.
- `sitemap`, `robots`, `manifest`, `icon`, `apple-icon`, `opengraph-image`, and
  `twitter-image` are entries.
- `proxy.ts` (16) and `middleware.ts` (15) exports are entries.
- `instrumentation.ts` and `instrumentation-client.ts` `register` are entries.
- `"use server"` async exports are entries.
- `"use client"` modules are entries only when imported, or when they are route
  convention files.
- Metadata and segment config exports are live: `metadata`, `generateMetadata`,
  `viewport`, `generateViewport`, `generateStaticParams`, `generateSitemaps`,
  `dynamic`, `dynamicParams`, `revalidate`, `fetchCache`, `runtime`,
  `preferredRegion`, `maxDuration`.
- Each entry records framework, kind, route, runtime, and certainty.
- No generic "looks like a page" heuristic.

**Read:** [`../docs/03-algorithms.md`](../docs/03-algorithms.md) §9.1.

**Evidence:** `fixtures/next-app-router/`; corpus entries for each convention.

### 1.8 — Entry adapters: Pages Router and React

**Task:** Detect Pages Router pages, API routes, `_app`, `_document`, `_error`,
data functions, React roots, route configs, and lazy imports.

**Acceptance:**

- `pages/**` default exports are entries.
- `pages/api/**` HTTP method exports are entries.
- `getServerSideProps`, `getStaticProps`, `getStaticPaths`, and
  `getInitialProps` are live.
- `createRoot` and `hydrateRoot` roots are entries.
- Route configuration objects are entries.
- `lazy(() => import(...))` creates an edge to the imported module.
- A test proves a page is never a candidate.

**Read:** [`../docs/03-algorithms.md`](../docs/03-algorithms.md) §9.2–§9.3.

**Evidence:** `fixtures/next-pages-router/`, `fixtures/react-spa/`.

### 1.9 — Reachability and witnesses

**Task:** Implement multi-source BFS over the over-approximate graph, with parent
pointers and witness reconstruction.

**Acceptance:**

- BFS starts from all entries, sorted by `EntryId`.
- Outgoing edges are traversed in sorted target order.
- `probable` and `unknown` edges are traversed.
- Every live symbol has a shortest witness.
- The witness is a valid path; a test verifies each edge exists.
- A cycle does not loop forever.
- A dead symbol has a proof trace with the edges considered and the unknown
  edges that block it.

**Read:** [`../docs/03-algorithms.md`](../docs/03-algorithms.md) §10.

**Evidence:** invariant tests (`F1`–`F3`); proof fixtures.

### 1.10 — Dynamic-edge abstraction

**Task:** Implement bounded dynamic scopes for literal, template, glob,
namespace, string-keyed, and computed access.

**Acceptance:**

- Literal `import()` and `require()` create a certain edge.
- A template with a static prefix creates a probable edge to the matching files.
- A variable import inside a package creates an unknown edge scoped to the
  package.
- A variable import at the repo root creates a repo-scope unknown edge and
  refuses deletion.
- `obj[key]` with literal keys creates probable edges; a computed key creates an
  unknown edge scoped to the module.
- `globalThis[name]` creates a repo-scope unknown edge.
- Unknown edges are never pruned and appear in the proof trace.
- Fixtures cover each form, including a false-positive trap.

**Read:** [`../docs/03-algorithms.md`](../docs/03-algorithms.md) §11.

**Evidence:** `fixtures/dynamic-edges/`; corpus entries.

### 1.11 — SCC condensation

**Task:** Implement iterative Tarjan over the symbol graph and expose SCC IDs and
members.

**Acceptance:**

- Deep graphs do not overflow the stack.
- SCC IDs are deterministic (sorted by minimum `SymbolId`).
- Every symbol belongs to exactly one SCC.
- The condensation DAG has no cycles.
- A test proves no partial SCC can be produced by the closure helpers.
- Cross-check against a reference implementation on random small graphs
  (`proptest`).

**Read:** [`../docs/03-algorithms.md`](../docs/03-algorithms.md) §15.

**Evidence:** SCC unit tests; property tests.

### 1.12 — Snapshot cache and warm path

**Task:** Implement a content-addressed cache for parse summaries, resolution,
and snapshots.

**Acceptance:**

- Cache key is `(file hashes, configHash, resolverHash, toolVersion,
  protocolVersion)`.
- Atomic writes (temp file + rename).
- A corrupt entry is discarded and recorded; it is never trusted.
- A warm scan skips unchanged files and produces the same snapshot bytes.
- A failed parse is cached as a failure with its diagnostic.
- The cache contains no source text beyond names and spans.
- `shrike cache clean` and `shrike cache verify` work.

**Read:** [`../docs/01-architecture.md`](../docs/01-architecture.md) §8.

**Evidence:** cache tests; warm/cold snapshot equality.

### 1.13 — `shrike-engine` binary and commands

**Task:** Implement `snapshot`, `proof`, `evidence`, `version`, and the protocol
loop.

**Acceptance:**

- `snapshot` returns a schema-valid `GraphSnapshot` summary.
- `proof` returns a witness for a live symbol or a proof trace for a dead one,
  with an exit code of 0 or 1.
- `evidence` is a stub that returns `unsupported` until PLAN-4.
- `version` returns tool and protocol versions.
- A malformed request is a protocol error, exit 4.
- The engine never executes project code or edits the tree.

**Read:** [`../docs/01-architecture.md`](../docs/01-architecture.md) §6.

**Evidence:** contract tests; CLI integration tests.

### 1.14 — Determinism harness

**Task:** Add a harness that runs the same scan twice and with different thread
counts, then compares serialized snapshots.

**Acceptance:**

- 1, 2, and 8 threads produce byte-identical output.
- Cold and warm produce the same logical output (cache metadata excluded from
  the compared payload).
- Files discovered in a different filesystem order produce the same output.
- A deliberately nondeterministic implementation fails the harness (negative
  test).

**Read:** [`../docs/03-algorithms.md`](../docs/03-algorithms.md) §24.

**Evidence:** CI log; harness tests.

### 1.15 — Performance harness

**Task:** Add `benchmarks/harness` and record 1k and 10k cold/warm time and peak
RSS.

**Acceptance:**

- 3 warmups, 5 measured runs.
- Hardware, versions, and corpus commit recorded.
- Raw JSON stored under `benchmarks/reports/`.
- 1k cold ≤ 1.5 s, 1k warm ≤ 400 ms, 10k cold ≤ 6 s, 10k warm ≤ 2 s, 10k peak
  RSS ≤ 600 MB.
- A missed budget is recorded with the measurement and one optimization attempt.

**Read:** [`../docs/08-benchmarks-and-confidence.md`](../docs/08-benchmarks-and-confidence.md) §4.

**Evidence:** benchmark report in the tracker.

### 1.16 — Fail-closed errors

**Task:** Map engine failures to exit 4 and structured errors.

**Acceptance:**

- Missing binary → exit 4.
- Malformed response → exit 4.
- Timeout → exit 4.
- Crash → exit 4.
- Partial result → exit 4.
- No deletion is proposed after any of these.

**Read:** [`../docs/01-architecture.md`](../docs/01-architecture.md) §10.

**Evidence:** integration tests.

### 1.17 — CLI integration: `scan` and `proof`

**Task:** Wire the TypeScript CLI to the engine for `scan` and `proof`.

**Acceptance:**

- `shrike scan` prints a human summary and `--json` matches the fixture oracle.
- `shrike proof <symbol>` prints a witness or a proof trace.
- Exit codes match the contract: 0 clean, 1 findings, 2 usage, 4 engine error.
- The CLI reads structured output, not just the exit code.
- A missing engine fails closed with a clear message.

**Read:** [`../docs/07-agent-and-distribution.md`](../docs/07-agent-and-distribution.md) §7.

**Evidence:** CLI integration tests.

---

## 5. Exit gate

- [ ] The engine parses and indexes a 1,000-file Next.js app.
- [ ] Every fixture in `fixtures/next-app-router/`, `next-pages-router/`,
      `react-spa/`, `dynamic-edges/`, and `monorepo/` passes.
- [ ] Every live symbol has a valid shortest witness.
- [ ] Every dead symbol has a proof trace naming blocking unknown edges.
- [ ] SCC tests and property tests pass.
- [ ] Determinism harness passes at 1, 2, and 8 threads.
- [ ] Performance budgets are met or the miss is recorded.
- [ ] The must-not-report corpus is green.
- [ ] `scan` and `proof` work through the CLI on a real app.
- [ ] No engine path executes project code or edits the tree.

---

## 6. Risks and mitigations

| Risk | Mitigation |
|---|---|
| `oxc` API churn | Pin the version; isolate behind `shrike-syntax`; upgrade behind fixtures |
| Resolver edge cases | Fixture-first; explicit unresolved diagnostics; package `exports` tests |
| Memory blowup from summaries | Intern strings; drop ASTs; CSR; measure RSS per phase |
| Witness cost | Parent pointers during BFS; no per-symbol path search |
| Entry false positives | Framework fixtures and corpus; no heuristics |
| Determinism regressions | Harness in CI; sorted arenas; no map iteration in output |
| Performance scope creep | Timebox; ship correctness first; optimize after profiling |

---

## 7. Definition of done

A reviewer can run `shrike scan --json` on a real Next.js app, inspect the
snapshot, and ask `shrike proof` for any symbol and receive either a witness path
or a proof trace that names the unknown edges blocking deletion — deterministically
and within the performance budget.
