# 01 — Architecture

> **Parent:** [MASTER_PROMPT.md](../MASTER_PROMPT.md) · **Tracker:** [TRACKER.md](../TRACKER.md)
> **Status:** canonical · Authored 2026-11-20

---

## 1. Goals

1. Hold two whole-repository graph snapshots (merge base and head) at once.
2. Diff reachability and compute cascade closures in seconds, not minutes.
3. Parse and index 10,000 files within the performance budget.
4. Produce a reviewable deletion PR with independent verified batches.
5. Emit a content-addressed receipt for every batch.
6. Fail closed: an unknown edge, a missing check, or an unavailable engine
   blocks deletion.
7. Keep the analyzer deterministic: same input, same IDs, same output.
8. Ship as a single `shrike` CLI installed from npm, with a native Rust engine.

---

## 2. Constraints

- Target projects are Node.js projects. Node is present at verification time.
- `shrike.config.ts` is executable TypeScript, so config evaluation needs a
  JavaScript runtime.
- The TypeScript 7 semantic API is a stdio server, not an in-process JS API, so
  a TypeScript host does not buy in-process type checking.
- `oxc` is the parse and semantic substrate. Do not build a second parser.
- The verifier runs the project's own scripts, which are trusted by definition.
- The engine never executes project code.
- The product is greenfield: no legacy code or schema compatibility.

---

## 3. Language decision

### 3.1 Rust engine

The Rust engine owns the analysis hot path:

- parsing and per-file semantic summaries (`oxc_parser`, `oxc_semantic`);
- symbol interning and the module graph;
- entry adapters and reachability with witness paths;
- SCC condensation, delta, cascade closure, batch partitioning;
- build-graph ingestion and the evidence join;
- the persistent snapshot cache.

Why Rust:

- **Graph workload.** Two snapshots, a reachability diff, SCCs, closures, and
  batch partitioning are memory-heavy. A TypeScript host pays for AST retention
  and GC on every snapshot.
- **Memory.** The 10,000-file budget is 600 MB with two snapshots. Retaining
  JavaScript ASTs blows it; arena-allocated Rust summaries do not.
- **Determinism.** Interned IDs and arenas remove the object-identity and
  iteration-order variance that makes JS graph output hard to reproduce.
- **Parallelism.** `rayon` over files, with a deterministic merge, is
  straightforward.
- **Startup.** A native binary does not pay Node startup or JIT warmup on the
  hook path.

What Rust does not buy: type checking, executable config, or product sense.
Do not pretend otherwise.

### 3.2 TypeScript host

The TypeScript host owns:

- the `shrike` CLI and command surface;
- loading and executing `shrike.config.ts`;
- detecting and running project checks;
- capturing baselines and comparing failure signatures;
- classifying flaky and known failures;
- applying and reverting snapshots;
- orchestrating bisection;
- writing receipts and rendering PR comments;
- hooks, the GitHub Action, and the agent bundle;
- the MCP wrapper.

Why the host stays TypeScript:

- the target project's toolchain is Node;
- the config is executable TypeScript;
- verification is process orchestration, not graph computation;
- npm is the natural install path;
- the host can evolve without recompiling the engine.

### 3.3 What we rejected

| Option | Why not |
|---|---|
| TypeScript-only engine | AST retention and GC make two-snapshot diffing and the 10k-file memory budget the wrong place to compete |
| Pure Rust CLI | Config execution and the verifier need Node; embedding a JS runtime adds complexity for no product gain |
| Rust core with an FFI verifier | Tight coupling, crash risk, and no benefit over a process boundary |
| WASM core in Node | Keeps the JS memory ceiling and adds a boundary |
| TypeScript semantic service | The TS 7 API is already a stdio server; the double-parse cost is measurable |
| A hosted analysis service | Violates the local-first, no-source-upload trust model; a later option only |

---

## 4. Component diagram

```text
┌───────────────────────────────────────────────────────────────┐
│ shrike (TypeScript CLI)                                       │
│                                                               │
│  config ──► checks ──► baseline ──► apply ──► compare         │
│                                      │            │           │
│                                      ▼            ▼           │
│                                   revert      receipt + PR    │
│                                                               │
│  commands: scan, delta, plan, proof, prune, verify, receipt   │
└──────────────────────────┬────────────────────────────────────┘
                           │ JSON contract (stdin/stdout)
┌──────────────────────────▼────────────────────────────────────┐
│ shrike-engine (Rust binary)                                   │
│                                                               │
│  loader ──► parse ──► semantic ──► resolve ──► graph          │
│                                      │            │           │
│                                      ▼            ▼           │
│                                   entries ──► reachability    │
│                                      │            │           │
│                                      ▼            ▼           │
│                                   delta ◄──── provenance     │
│                                      │                        │
│                                      ▼                        │
│                              SCC + cascade + batches          │
│                                      │                        │
│                                      ▼                        │
│                              evidence join (optional)         │
└───────────────────────────────────────────────────────────────┘
```

The host owns the command surface and everything that executes project code.
The Rust engine is invoked for analysis and returns structured results. The host
never trusts the engine's exit code alone.

---

## 5. Crates and packages

### 5.1 Rust workspace

| Crate | Responsibility | Depends on |
|---|---|---|
| `shrike-core` | IDs, interners, spans, diagnostics, config types, hashing, errors | — |
| `shrike-protocol` | request/response types, line-delimited codec, versioning | `shrike-core` |
| `shrike-syntax` | `oxc` parse + semantic, per-file summary | `shrike-core` |
| `shrike-resolve` | module resolution: relative, alias, workspace, package exports, barrel | `shrike-core`, `shrike-syntax` |
| `shrike-graph` | snapshot, module graph, symbol graph, entries, reachability, witnesses, SCC | `shrike-syntax`, `shrike-resolve` |
| `shrike-delta` | base/head comparison, orphan delta, trigger attribution, cascade closure | `shrike-graph` |
| `shrike-plan` | conflict graph, batch partitioning, blast radius, impact, bisection support | `shrike-delta` |
| `shrike-evidence` | build-graph ingestion, module join, byte attribution, evidence status | `shrike-graph` |
| `shrike-receipt` | receipt model, content-addressed IDs, schema validation, signing hook | `shrike-core` |
| `shrike-engine` | binary entry point, command dispatch, output | all crates |
| `xtask` | dev tasks: fixtures, schemas, bench, release, docs check | workspace |

### 5.2 TypeScript packages

| Package | Responsibility |
|---|---|
| `packages/cli` | `shrike` binary, command parsing, config loading, output, exit codes |
| `packages/verifier` | baseline, checks, worktree, apply/revert, comparison, bisection |
| `packages/receipts` | receipt writer, PR renderer, Markdown/HTML summaries |
| `packages/agent` | hook installer, bounded bundle, MCP wrapper |
| `packages/action` | GitHub Action wrapper |
| `packages/engine-*` | platform-specific native binary packages |
| `packages/testkit` | fixture harness, temp repos, snapshot assertions |

### 5.3 Shared assets

| Path | Contents |
|---|---|
| `schemas/` | JSON Schemas for protocol, snapshot, delta, plan, receipt, config |
| `fixtures/` | synthetic repositories and expected outputs |
| `benchmarks/` | corpus, harness, raw results |
| `scripts/` | repo automation, schema validation, corpus runner, release |
| `docs/`, `plans/`, `prompts/`, `research/` | this design package |

Full layout rationale: [02-folder-structure.md](02-folder-structure.md).

---

## 6. Engine protocol

The protocol is versioned line-delimited JSON over stdin/stdout.

### 6.1 Request

```json
{
  "protocolVersion": "1.0",
  "id": "req_01J...",
  "command": "delta",
  "payload": {
    "root": "/repo",
    "base": "origin/main",
    "head": "HEAD",
    "config": {
      "include": ["src/**"],
      "ignore": [".next/**", "node_modules/**"],
      "tsconfig": "tsconfig.json",
      "entryPolicy": "production",
      "batchPolicy": {
        "maxSymbols": 50,
        "maxFiles": 10,
        "maxLiveGraphPercent": 5
      }
    },
    "cacheDir": ".shrike/cache"
  }
}
```

### 6.2 Response

```json
{
  "protocolVersion": "1.0",
  "id": "req_01J...",
  "ok": true,
  "result": {
    "snapshotHash": "sha256:...",
    "delta": {
      "newlyDeadSymbols": [
        {
          "id": "src/lib/format.ts#formatLegacyDate",
          "reason": "caller removed",
          "trigger": "src/app/invoice/page.tsx",
          "witness": [
            "app/invoice/page.tsx#default",
            "src/features/invoice/InvoiceCard.tsx#InvoiceCard",
            "src/lib/format.ts#formatLegacyDate"
          ]
        }
      ]
    },
    "batches": [
      {
        "id": "batch_1",
        "files": ["src/lib/format.ts"],
        "symbols": ["src/lib/format.ts#formatLegacyDate"],
        "expectedEffect": "Remove 1 dead symbol"
      }
    ]
  },
  "error": null
}
```

### 6.3 Rules

- One request per line; one response per line.
- The host never trusts the engine's exit code alone; it reads the structured
  result.
- A malformed response is a protocol error and fails closed.
- The engine never executes project code, runs checks, or edits the tree.
- The host never edits files outside the worktree or the plan.
- `unknown` edges are returned explicitly; an omitted value is a protocol
  failure, not "no unknown edges."
- The contract is versioned independently of the tool version; a breaking change
  bumps `protocolVersion` and invalidates cached plans.
- The contract is covered by round-trip fixtures: valid, malformed, timeout,
  engine-crash, and partial-result cases.

Full schema: [`../schemas/protocol.schema.json`](../schemas/protocol.schema.json).

---

## 7. Data model

### 7.1 IDs

All IDs are interned `u32` handles scoped to a snapshot.

| ID | Key |
|---|---|
| `FileId` | normalized relative path |
| `SymbolId` | file, span start, kind, name |
| `ModuleId` | resolved module path |
| `EntryId` | framework entry key (route, export, convention) |
| `EdgeId` | source, target, kind, certainty |

IDs must be stable across runs on the same input. Do not use pointer addresses,
hash-map iteration order, or wall-clock values.

### 7.2 Certainty

| Certainty | Meaning | Effect on deletion |
|---|---|---|
| `certain` | Resolved import, static reference, framework convention | Normal |
| `probable` | Heuristic match, barrel ambiguity, template prefix | Blocks unless the candidate is in the same SCC and has no external referrer |
| `unknown` | Computed access, unresolved dynamic import, reflection-like pattern | Blocks deletion of every symbol it could reach |

`unknown` edges are a feature. They are how the tool stays honest.

### 7.3 Snapshot

```text
GraphSnapshot
  files:      FileId -> FileRecord
  symbols:    SymbolId -> SymbolRecord
  modules:    ModuleId -> ModuleRecord
  entries:    EntryId -> EntryRecord
  edges:      Vec<Edge>
  configHash: string
  resolverHash: string
  toolVersion: string
  protocolVersion: string
```

`FileRecord` holds the path, content hash, parse status, module kind, and
workspace package. `SymbolRecord` holds the name, kind, span, export status,
owner, and entry roles. `EntryRecord` holds the framework, reason, route,
runtime, and certainty.

### 7.4 Witness

A `Witness` is a shortest path from an entry to a live symbol:

```text
Witness {
  entry: EntryId,
  path: Vec<(SymbolId, EdgeId)>,
  length: u32
}
```

Witnesses are computed during BFS and stored for every live symbol. They power
`shrike proof`, the delta explanation, and the agent bundle.

### 7.5 Proof trace

For a dead symbol, the proof trace is the absence of a path under the
abstraction, plus the list of edges considered and rejected, and the `unknown`
edges that would block deletion. It is an auditable trace, not a formal theorem.

Full schema: [`../schemas/graph-snapshot.schema.json`](../schemas/graph-snapshot.schema.json).

---

## 8. Caching

### 8.1 Layers

| Layer | Key | Stored |
|---|---|---|
| Parse | file content hash + parser version | per-file summary, spans, directives |
| Resolve | importer + specifier + resolver config hash | resolved module ID |
| Snapshot | file hashes + config + resolver + tool version | serialized `GraphSnapshot` |
| Build graph | snapshot hash + Next.js version | joined module map |
| Plan | base/head snapshot hashes + policy | `DeletionPlan` |

### 8.2 Invalidation

- Any file content change invalidates that file and its importers' resolve
  entries.
- A config or resolver change invalidates the whole snapshot.
- A tool or protocol version change invalidates everything.
- A failed parse is cached as a failure with the diagnostic, not retried on
  every run.

### 8.3 Persistence

Cache lives in `.shrike/cache/` by default, with a content-addressed layout. It
is safe to delete. It must never contain source text beyond what a diagnostic
needs, and it must be excluded from the repository's VCS.

`shrike cache clean` removes it. `shrike cache verify` checks integrity.

---

## 9. Concurrency

- Parse and per-file semantic extraction run in a `rayon` pool.
- Results are merged in deterministic file order, not completion order.
- Graph construction is single-threaded until the edge set is complete; then
  reachability and SCC work can run in parallel per entry or per component.
- The cache is written atomically (temp file + rename).
- The host runs checks sequentially by default; parallel checks are opt-in and
  must not share a worktree.
- The engine is one shot per analysis command by default. A persistent process is
  a later optimization only if startup or cache loading is measured as
  significant.

Determinism tests run the same scan twice and compare the serialized snapshot
byte for byte.

---

## 10. Error handling

| Situation | Behavior |
|---|---|
| File cannot be parsed | Record a diagnostic; mark the file `unparsed`; block deletion of symbols in it |
| Import cannot be resolved | Record an `unresolved` edge; report it; do not treat the target as dead |
| Dynamic edge is unknown | Add an `unknown` edge; block the candidate set |
| Engine binary missing | Fail closed; exit 4 |
| Engine protocol error or crash | Fail closed; exit 4 |
| Config cannot load | Fail closed; exit 4 |
| Check command missing | Fail closed; exit 4 |
| Check times out | Classify as `timeout`; fail the batch |
| Verifier crashes | Fail closed; leave the worktree; print its path; exit 4 |
| Revert fails | Exit 5; leave a recovery snapshot and a loud message |
| Build snapshot malformed | Ignore build evidence; continue with static evidence; record why |
| Cache corrupt | Discard the cache entry; rescan; record |

The principle: **a missing fact is not evidence of death.**

---

## 11. Security

- Never execute project code during scan. The Rust engine reads files only.
- Config execution is isolated in the host with a child-process boundary and a
  timeout.
- Redact absolute paths, home directories, environment values, and tokens from
  logs and receipts.
- The verifier runs project scripts, which are trusted by definition; document
  that trust boundary.
- The hook must not send source code anywhere. It writes local JSON only.
- The PR renderer escapes Markdown and truncates logs.
- No network calls from the analyzer. The build-graph reader reads local files.
- Receipts are content-addressed; optional signing is a later feature.
- Supply chain: signed binaries, checksums, npm provenance, `cargo audit`, and
  pinned dependencies.

Full design: [09-security-and-trust.md](09-security-and-trust.md).

---

## 12. Performance budgets

| Workload | Target |
|---|---|
| 1,000 files, cold scan | ≤ 1.5 s |
| 1,000 files, warm scan | ≤ 400 ms |
| 10,000 files, cold scan | ≤ 6 s |
| 10,000 files, warm scan | ≤ 2 s |
| 10,000 files, peak RSS | ≤ 600 MB |
| Delta on a 1,000-file repo | ≤ 1 s warm |
| Fast receipt (delta + typecheck + lint) | ≤ 60 s |
| Full receipt (typecheck + lint + test + build) | ≤ 10 min |

Record actuals in [TRACKER.md](../TRACKER.md) §5. If a budget is missed, record
why before changing the target.

---

## 13. Distribution

- The CLI is an npm package with a small TypeScript launcher.
- The native engine is distributed as optional platform packages:
  `@shrike/engine-linux-x64-gnu`, `@shrike/engine-linux-arm64-gnu`,
  `@shrike/engine-darwin-arm64`, `@shrike/engine-win32-x64-msvc`, and so on.
- The launcher resolves the binary for the current platform, verifies its
  checksum, and spawns it.
- A missing or incompatible binary fails closed for analysis commands.
- The binary is statically linked where practical; glibc/musl variants are
  separate packages.
- Release artifacts are signed and checksummed; npm publishing uses provenance.
- A standalone static binary is also published for users who do not want npm.

Full release plan: [`../plans/PLAN-5-distribution-and-launch.md`](../plans/PLAN-5-distribution-and-launch.md)
and [`../plans/PLAN-6-hardening-and-scale.md`](../plans/PLAN-6-hardening-and-scale.md).

---

## 14. Open questions

| ID | Question | Owner |
|---|---|---|
| Q-001 | Does the Rust engine own the CLI? | Resolved: no; TypeScript host |
| Q-009 | Is the snapshot format stable across minor versions? | PLAN-1 |
| Q-010 | How are monorepo workspace boundaries represented? | PLAN-2 |
| Q-011 | Persistent engine process or one shot per command? | PLAN-1 |
| Q-023 | Native binary or WASM? | PLAN-1 (native first) |
| Q-024 | Which platforms are supported at first? | PLAN-5 |
| Q-029 | How is the blast-radius threshold configured? | PLAN-2 |
| Q-030 | Do receipts include a human summary and machine JSON? | PLAN-3 |
