# How it works

This is the algorithm and internals guide. For the user-facing catalogue, read
[features.md](features.md); for crate/package boundaries, read
[architecture.md](architecture.md).

## The pipeline at a glance

| Phase | What happens |
| --- | --- |
| A — Inventory | Discover files, hashes, config, workspace layout; sorted and deduplicated |
| B — Parse | `oxc` parse, then a per-file semantic summary (no AST retained) |
| C — Resolve | Relative, tsconfig alias, workspace, package export, and barrel resolution |
| D — Graph | Module + symbol graph, edges with certainty (`certain`/`probable`); unknown edges are recorded separately |
| E — Entries | Seed liveness from framework, package, test, and config conventions |
| F — Reachability | Multi-source BFS with witness parent pointers |
| G — Candidates | Live set, dead set, proof traces |
| H — Snapshot | Deterministic serialization + content-addressed cache |
| I — Delta | `live(base) − live(head)` |
| J — Trigger | Lost witness + changed edge attribution |
| K — SCC | Tarjan condensation of cyclic groups |
| L — Cascade | Closure over the SCC DAG, stopping at every safety boundary |
| M — Impact | Files, lines, routes, bytes |
| N — Partition | Conflict graph + greedy coloring + batch cap |
| O — Blast radius | Split or refuse oversized/risky batches |
| P — Patches | Materialize deletion edits |
| Q — Verify | Host: baseline → apply → check → compare |
| R — Bisect | Split failures, isolate the culprit, retry the remainder |
| S — Receipt | Content-addressed proof for each batch |

A–H run once per snapshot. I–P run on the base/head pair. Q–S run in the
TypeScript host. The engine and host only exchange JSON through
[`schemas/`](../schemas/).

## Data model

`shrike-core` defines `u32` handle types (`FileId`, `SymbolId`, `ModuleId`, ...),
but the graph works with **stable string keys** — `file#name` for symbols and
normalized paths for files — so snapshots remain comparable across processes.
No pointer addresses, hash-map iteration order, or timestamps enter a key.

- Records: `FileRecord` (path, content hash, parse status, module kind, package,
  generated flag, lines), `SymbolRecord` (key, name, kind, file, span, exported,
  entry, public API, generated, type-only, lines), `ModuleRecord`, `EntryRecord`,
  `EdgeRecord`, `UnknownRecord`, `WitnessRecord`, `ProofRecord`, `Span`.
- Storage: record vectors plus `BTreeMap` indexes for lookup. During reachability,
  adjacency is held in `HashMap`s with sorted adjacency lists and a `HashSet` +
  `VecDeque` worklist; results are rebuilt as sorted `BTreeSet`s for the snapshot.
- Snapshot: sorted files, symbols, modules, entries, edges, and SCCs, plus live
  and candidate keys, `unused_files`, `internal_only`, witnesses, proofs,
  unknowns, diagnostics, and `graph_hash` (BLAKE3 over canonical JSON). The cache
  key hashes `ANALYSIS_VERSION`, the snapshot schema, tool and protocol versions,
  config hash, resolver hash, and every file hash.

### Edge certainty

`EdgeRecord.certainty` is `certain` or `probable`. Static imports and re-exports,
references, JSX uses, and literal dynamic imports are `certain`; glob matches and
other bounded dynamic edges are `probable`.

Non-local access is recorded separately as an `UnknownRecord` with a scope —
`module` (one file's exports), `package`, or `repo` — not as a normal edge. The
reachability pass widens the live set for each unknown: module scope makes that
module's exports live, package scope makes that package's symbols live, and repo
scope makes every symbol live. A widened symbol is seeded into the queue like an
entry so its own references stay live. A dynamic value that cannot be placed
widens instead of guessing.

## 1. Inventory, parse, resolve

Inventory normalizes paths to `/` separators and records content hashes. Parsing
uses `oxc` and keeps only summaries: declarations, spans, imports/exports,
directives, JSX uses, dynamic forms, component props, enum/class/namespace
members. A parse failure is a diagnostic; **unparsed files never yield
candidates**. Generated files and public API surfaces are recorded and excluded
by policy.

Resolution covers relative paths, tsconfig paths and aliases, workspace
packages, `package.json` `exports`/`main`/`bin`, barrel chains, and platform
variants (for example Metro's `ios`/`android`/`native` order). Unresolved
specifiers are recorded, never silently dropped.

## 2. Entries: what keeps code alive

Liveness is seeded from all of these, not just from static `main` functions:

- package public surface (`exports`, `main`, `bin`, index barrels, declaration
  files);
- framework conventions — Next.js App/Pages Router files, React roots,
  middleware, instrumentation, server actions, route segment config, and the
  equivalent conventions of other supported routers and tooling;
- config and test modules named by manifests or explicit config;
- every symbol widened by an unknown scope.

Framework rules are gated on the owning package declaring the dependency, so a
`pages/` directory in a React-only package does not accidentally stay live.
Framework-invoked *exports* are scoped per file type: a Next.js `generateMetadata`
is live on a page, but a copy on an unrelated module remains analyzable.

## 3. Reachability and witnesses

```text
entries = all entry records, sorted by symbol key
queue = every entry symbol, plus symbols widened by an unknown scope
mark each seeded symbol live
while queue not empty:
    s = dequeue()
    for edge in outgoing(s) sorted by (target, kind):
        if not live[edge.target]:
            live[edge.target] = true
            parent[edge.target] = (edge.source, edge.kind)
            enqueue(edge.target)
```

The BFS traverses every recorded edge (`certain` and `probable`), while
`unknowns` widen the seed set; widening never changes the reachability rule, so
a live export's helpers stay live too. Parent links are enough to reconstruct a
shortest witness path from an entry. Key invariants:

- every live symbol has a witness (a shortest path from an entry);
- every witness is a valid path and shortest in edge count;
- no entry, no unparsed symbol, and no public API symbol is a candidate;
- a dead named re-export alias is reported even when its target is dead;
- an unused file is reported only when no entry names it, no other module edges
  into it, its module symbol is dead, and it parsed successfully.

For every candidate the engine records a **proof trace**: the symbol, incoming
and outgoing edges, and the reason it is not live (`no witness from any entry`,
`no witness; isolated symbol`, `blocked by public API`, `blocked by generated
file`). It is
an auditable record of the abstraction, not a formal theorem. Use
`shrike proof <symbol>` to inspect one.

## 4. Dynamic edges

Dynamic access is the main source of false positives, so it is handled from
most to least precise:

| Form | Scope | Effect |
| --- | --- | --- |
| literal `import()` / `require()` | module | `certain` edge to the target |
| static prefix / template | directory | `probable` edges to all matches |
| `import.meta.glob`, `require.context` | glob / directory | all matched modules' exports live; excludes honored |
| literal namespace member reads, destructuring, `.then` callbacks | export names | narrow to the actually accessed names |
| static worker / worklet URL | module | live module edge |
| computed access or non-literal dynamic import | package → repo | widen; repo scope blocks deletion |

A pattern that cannot be placed resolves to repo scope rather than guessing.
Bundler globs follow the importer: `./` and `../` are relative to the importing
file, `/` is repo root, and tsconfig aliases resolve through the same map as
static imports.

## 5. Snapshots and the orphan delta

A snapshot is serialized once per commit. The delta is then:

```text
assert comparable(base, head)
orphaned = live(base) − live(head)
```

**This is not "all dead code at head".** It is code that was live at the base,
is not live at the head, is not an entry or public API at head, and whose files
parsed at both commits. Pre-existing debt is a `scan` concern, not a delta
finding. Comparability requires the same config hash, resolver hash, protocol
version, tool version, and snapshot schema version; otherwise the result is
`incomparable` with the changed input named.

For each orphan, the engine takes its base witness, diffs base/head edges, and
finds the first missing or changed edge on the witness path. The changed edge's
owner is the trigger (for example `removed-import`). If no single edge explains
it, the closest changed ancestor is named and the trigger is `indirect`. A
no-op change produces an empty delta.

Edge cases are explicit: different config → `incomparable`; missing base
snapshot → fail closed; dirty tree → refused unless `--allow-dirty` (the hook
sets it and marks its brief `dirty`); renamed export →
old name is the delta with the removed import/re-export as trigger (the
`renamed-export` fixture asserts `removed-import`); a barrel dropping a re-export →
underlying symbol is orphaned if no other path keeps it live; cycles → SCCs are
recorded in the snapshot, and their dead symbols are handled by the candidate
closure rather than deleted one at a time.

The delta JSON carries base/head commits, `newlyDead` items with their
`lastWitness` and trigger, `newlyDeadFiles`, cascade totals, and batches. See
[`schemas/delta.schema.json`](../schemas/delta.schema.json).

## 6. SCCs, cascade closure, impact

The graph pass computes SCCs with an iterative Tarjan implementation and stores
them (sorted) in the snapshot for diagnostics and proof. Deletion units are
declaration file groups joined by re-export edges, not SCCs.

Cascade closure starts from the delta's newly dead symbols and walks **reverse
edges**: any candidate symbol that referred to a dead symbol joins the closure,
and the walk repeats to a fixed point. Live symbols, entries, public API
symbols, generated files, and non-candidates are never added — the reachability
pass has already excluded them — so a component and its exclusive helper are
removed together instead of leaving a dangling import.

Impact reports symbols, files touched, line estimates from the orphan spans
(not whole-file line counts), affected routes, and bytes from build evidence
only. Shared chunks are attributed once; a single double-counted "total bytes
saved" is never reported.

## 7. Batches

The plan groups selected declarations by file, then attaches every dead named
re-export to its target declaration, transitively through barrel chains. This
keeps a declaration with its same-file users and with every dead specifier that
names it, without merging every target of a large barrel into one batch. Units
are sorted and packed into batches under `maxSymbols` (default 50) and
`maxFiles` (default 10); adding the next unit would exceed either cap flushes
the current batch and starts a new one. A declaration file is never split
across batches; a barrel file can contribute specifiers to several batches.
Batch order is deterministic, and the delta summary uses the same unit packing.

`shrike.config.ts` also exposes `batchPolicy.maxLiveGraphPercent` (default 5)
and `allowOversized` (default false); both are recorded in the plan and receipt
policy. The enforced caps today are the symbol and file limits. Blocked symbols
(public API or generated) are listed in the plan rather than batched.

## 8. Verified deletion

The host owns the part the analyzer cannot prove:

```text
baseline → apply → check → compare → revert-or-keep
```

1. **Baseline** — record HEAD, dirty state, commands, working directories,
   environment, exit codes, durations, failure signatures, and tool versions.
   Valid for one head commit and one check set.
2. **Apply** — worktree mode by default (`git worktree add --detach`);
   `--in-place` is explicit and refuses a dirty tree without `--allow-dirty`.
   Each batch restores the tree and re-analyzes it before editing, so a file
   that contributes symbols to more than one batch is patched with current
   spans rather than the plan-time offsets. When every symbol of a file is
   deleted and its module is not live for side effects, the whole file is
   removed; deleting the only specifier of a re-export statement removes the
   statement instead of leaving `export { }`, and an import specifier that
   names a deleted symbol is removed when its file is retained for side
   effects.
3. **Check** — tiers: fast (typecheck, lint) for hooks, standard (+test) for
   local, full (+build) for CI/PR. Commands come from project scripts or
   `shrike.config.ts`; the verifier does not guess when config supplies one.
4. **Compare** — against the baseline, not against zero: pass/pass → `pass`,
   pass/fail → `improvement`, fail/same signature → `known-failure` (does not
   block), fail/different signature → `new-failure` (blocks), timeout/pass →
   `timeout` (blocks). A failure on rerun then pass is `flaky`.
5. **Revert or keep** — required regressions revert the batch; a batch with
   more than one unit is bisected by half, the culprit isolated, and the
   remainder retried. If both halves pass but the full batch fails, it is an
   `interaction-failure` and the whole batch is reverted. A failed revert exits
   `5` and prints the exact recovery command; the tree is never left
   half-applied.

Failure signatures normalize check output by stripping timings, ANSI, and temp
paths while keeping error type, message, file, line, and test name, then hashing
with SHA-256 (`sha256:`). A flaky check does not block but is recorded.

Exit codes: `0` clean, `1` findings, `2` usage/config, `3` verification failed
and reverted, `4` could not run, `5` revert failed.

## 9. Receipts

A receipt records base/head commits and graph hashes, plan and batch hashes,
symbols/files/lines/patches, each check with baseline status and reruns, the
outcome (`verified`, `rejected`, `known-failure`, `flaky`, `error`,
`revert-failed`, `interaction-failure`), rejected sets with failure signatures,
build evidence status and hash, environment, and policy. The receipt ID derives
from a SHA-256 hash of the canonical receipt, excluding `createdAt`, `id`, and
`contentHash`. Output is redacted
before it reaches the receipt. See
[`schemas/receipt.schema.json`](../schemas/receipt.schema.json).

## 10. Build-graph evidence

Optional, never a hard dependency. `next analyze export` (Next.js 16.4+) emits
Turbopack JSONL; the parser streams it into module, chunk, route, and
client-reference indexes. Paths are normalized: Turbopack `[project]/` prefixes
and proxy modules are unwrapped, loaders/query suffixes stripped, dependencies
and emitted assets classified as non-project, and remaining project modules
matched by exact path then longest suffix.

`joinCoverage = matched project modules / all project modules`. Full evidence
needs ≥ 0.95; 0.80–0.95 is partial; < 0.80 cannot confirm but may still suppress.

| Verdict | Effect |
| --- | --- |
| `present` in a client chunk on a live route | **Suppress** deletion (mandatory) |
| `server-only` | weaken; still needs source proof and a server check |
| `absent` from every joined chunk | **Confirm**, advisory only |
| `unknown`, `stale`, `malformed`, `unsupported`, `build-failed`, `none` | static-only with a recorded status |

If the build shows a module used that the static graph missed, the engine adds
a `build` edge from the nearest importer and re-runs reachability. Build edges
can only add liveness, never prove death. Byte impact is reported per route and
as `sharedBytes` separately; shared chunks are never double counted. The
fallback matrix (including `--require-build-evidence`, which turns statuses
other than `full` or `partial` into exit 4) is exercised by the build-graph
fixtures.

## 11. Determinism and caching

Determinism is enforced by sorting: files by normalized path, symbols by
`(file, span start, kind, name)`, edges by `(source, target, kind, certainty)`,
entries by key, BFS adjacency by target, SCCs by minimum symbol, batches by
impact then minimum symbol. Parallel parse results are merged in file order.
No hash-map iteration order, host paths, or timestamps reach artifacts. Graph
and snapshot hashes use BLAKE3 (`blake3:`); receipt content hashes use SHA-256
(`sha256:`). CI runs the same analysis twice and diffs the JSON.

The snapshot cache key hashes the analysis version, snapshot schema, tool and
protocol versions, config, resolver, and file hashes. **Any change to analysis output must bump `ANALYSIS_VERSION` in
`crates/shrike-engine/src/cache.rs`**, otherwise a stale snapshot can survive.
See [testing.md](testing.md) for the determinism gate and cache troubleshooting.

## Worked example: removed caller

```ts
// before: app/invoices/page.tsx imports InvoiceCard, which imports formatInvoice
// after: the PR removes the InvoiceCard import and deletes its only usage
```

1. Base snapshot: `formatInvoice` is live with witness
   `page#default → InvoiceCard → formatInvoice`.
2. Head snapshot: that path is gone and no other edge reaches the symbol.
3. Delta: `formatInvoice` is `newlyDead`, trigger `removed-import` owned by
   `app/invoices/page.tsx`, with the lost witness attached.
4. Cascade: `formatCents`, used only by `formatInvoice`, joins the closure; the
   whole `src/lib/currency.ts` file becomes removable.
5. Batches: closure symbols are grouped into re-export units and packed into
   no-overlap batches under the symbol/file caps.
6. Verification: batch 1 is applied in a worktree, typecheck/lint/test/build run
   against a baseline, pass → keep + receipt; a regression would bisect, revert
   the culprit, and retry the rest.

## Complexity

| Stage | Cost |
| --- | --- |
| Parse / resolve | O(source bytes) / O(specifiers), parallel by file |
| Graph, reachability, SCC, cascade, delta | O(symbols + edges) each |
| Cascade / batching | O(symbols + edges) reverse walk; O(symbols log symbols) sorting/grouping |
| Bisection | O(log batch × check cost); checks dominate |

Budgets and the benchmark harness are in [testing.md](testing.md). The key
invariants to preserve when changing any stage: no declaration file split
across batches, no declaration split from its dead re-export specifiers, no
entry or public API deletion, no unknown scope pruned, and a complete receipt
or no `verified` outcome.
