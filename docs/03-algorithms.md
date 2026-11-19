# 03 — Algorithms

> **Parent:** [MASTER_PROMPT.md](../MASTER_PROMPT.md) · **Tracker:** [TRACKER.md](../TRACKER.md)
> **Status:** canonical · Authored 2026-11-20

---

## 1. What this document owns

This is the single source of truth for how Shrike computes:

- the symbol and module graph;
- reachability and witnesses;
- the whole-graph PR orphan delta;
- the cascade closure;
- deletion batches and blast radius;
- verification bisection;
- the proof trace and receipt inputs.

If the implementation and this document disagree, the implementation is wrong
until this document is amended.

---

## 2. Design goals

1. **Soundness over recall.** A false negative is acceptable; a false positive is
   a release blocker.
2. **Explainability.** Every live symbol has a witness; every dead symbol has a
   proof trace; every delta item names the change that killed it.
3. **Determinism.** Same input, same IDs, same output, byte for byte.
4. **Bounded risk.** No deletion batch exceeds a configured blast radius.
5. **Speed.** Linear passes over the graph after parsing; two snapshots fit in
   the memory budget.
6. **Composability.** Each phase is a pure function over an explicit data model.

---

## 3. Pipeline

```text
A. inventory       files, hashes, config, workspace layout
B. parse           oxc parse + per-file semantic summary
C. resolve         relative, alias, workspace, package, barrel
D. graph           module graph + symbol graph + edge certainty
E. entries         framework seeds (Next.js, React, packages, tests)
F. reachability    multi-source BFS + witness parent pointers
G. candidates      live set, dead set, proof traces
H. snapshot        serialize + cache
I. delta           live(base) − live(head)
J. trigger         lost witness + changed edge attribution
K. scc             Tarjan condensation
L. cascade         closure over the SCC DAG
M. impact          files, lines, routes, bytes
N. partition       conflict graph + greedy coloring + batch cap
O. blast radius    split or refuse oversized batches
P. patches         materialize deletion patches
Q. verify          host: baseline → apply → check → compare
R. bisect          split failures; isolate culprit; retry remainder
S. receipt         content-addressed proof for every batch
```

Phases A–H run once per snapshot. I–P run on the pair. Q–S are host-side.

---

## 4. Data model

### 4.1 IDs and interners

All IDs are `u32` indices into typed arenas.

```text
FileId      = u32   // index into files
SymbolId    = u32   // index into symbols
ModuleId    = u32   // index into modules
EntryId     = u32   // index into entries
EdgeId      = u32   // index into edges
SccId       = u32   // index into SCCs
```

Interning rules:

- file key: normalized relative path, `/` separators, case-sensitive;
- symbol key: `(file, span.start, kind, name)`;
- module key: resolved absolute path, then relativized to the workspace root;
- entry key: `(framework, kind, route, exportName)`;
- edge key: `(source, target, kind, certainty)`.

Do not use pointer addresses, hash-map iteration order, or timestamps in an ID.

### 4.2 Records

```text
FileRecord {
  path: String,
  contentHash: [u8; 32],
  parseStatus: Ok | Error(diagnostic) | Skipped(reason),
  moduleKind: Ts | Tsx | Js | Jsx | Json | Declaration,
  package: Option<PackageId>,
  generated: bool
}

SymbolRecord {
  name: String,
  kind: Function | Class | Variable | Type | Interface | Enum | Namespace | Default | ReExport,
  file: FileId,
  span: Span,
  exported: bool,
  entryRoles: SmallVec<EntryId>,
  publicApi: bool
}

ModuleRecord {
  path: String,
  file: FileId,
  specifiers: Vec<Specifier>,
  resolved: Vec<ModuleId>,
  unresolved: Vec<UnresolvedSpecifier>
}

EntryRecord {
  framework: NextApp | NextPages | React | Package | Test | Config,
  kind: EntryKind,
  route: Option<String>,
  runtime: Client | Server | Both | Unknown,
  certainty: Certainty
}

Edge {
  source: SymbolId,
  target: SymbolId,
  kind: Import | ReExport | Reference | Call | Jsx | Dynamic | Unknown | Build,
  certainty: Certain
}

Span {
  start: u32,   // byte offset
  end: u32,
  line: u32,    // 1-based
  column: u32   // 1-based
}
```

### 4.3 Graph representation

- Symbols: `Vec<SymbolRecord>`.
- Outgoing edges: CSR (`offsets: Vec<u32>`, `edges: Vec<Edge>`).
- Incoming edges: CSR built once per snapshot.
- Live set: `FixedBitSet` over `SymbolId`.
- Candidate set: `FixedBitSet`.
- SCC: `Vec<SccId>` per symbol + `Vec<Vec<SymbolId>>` for members.
- Condensation DAG: CSR over `SccId`.

CSR is used because it is cache-friendly, deterministic, and cheap to serialize.

---

## 5. Phase A — Inventory

1. Read `shrike.config.ts` in the host; pass a normalized config to the engine.
2. Walk the include set; skip ignores, `.git`, `node_modules`, build outputs.
3. Normalize paths relative to the repository root.
4. Hash file contents with BLAKE3.
5. Detect workspace packages from `package.json` workspaces, `pnpm-workspace.yaml`,
   and `tsconfig` references.
6. Detect generated files from common markers and config; generated files are
   excluded from deletion candidates by default but still parsed for edges.
7. Sort files by normalized path. This order is the canonical merge order.

**Invariant A1:** the file list is sorted and unique.

---

## 6. Phase B — Parse and semantic summary

Use `oxc_parser` and `oxc_semantic`. Build a per-file summary; do not retain the
AST after the summary is built.

The summary captures:

- imports: source specifier, imported names, local names, type-only flag;
- exports: named, default, re-export, namespace, type-only;
- declarations: functions, classes, variables, types, interfaces, enums,
  namespaces;
- references: identifier reads/writes, JSX names, call expressions;
- directives: `"use client"`, `"use server"`;
- dynamic forms: `import()`, `require()`, `import x = require()`;
- JSX: component names, namespaced names, member expressions;
- framework exports: `metadata`, `generateMetadata`, `dynamic`, `revalidate`,
  `runtime`, `generateStaticParams`, `getServerSideProps`, `getStaticProps`, etc.;
- comments: only for suppression directives (`shrike-ignore`), not for analysis.

**Rule B1:** if a file fails to parse, record the diagnostic and mark it
`unparsed`. Symbols in an unparsed file are never deletion candidates.

**Rule B2:** do not infer semantics from text. Use the parser's AST and semantic
data. A regex-based import scanner is forbidden.

### 6.1 Semantic references

`oxc_semantic` provides scopes, symbols, and references. Use it to:

- bind local identifiers to declarations;
- distinguish a reference to a local from a reference to an import;
- find unresolved references (globals, ambient declarations, dynamic names);
- detect unused locals and imports as evidence, not as the product.

**Rule B3:** unresolved references are recorded, not dropped. They may indicate
a missing ambient type, a global, or a dynamic access.

---

## 7. Phase C — Resolution

Resolve, in order:

1. relative specifiers (`./`, `../`);
2. tsconfig `paths` and `baseUrl`;
3. workspace package names (`@repo/ui` → `packages/ui/src/index.ts`);
4. package `exports` / `main` / `module` / `types`;
5. barrel re-exports (`index.ts` re-exporting a module);
6. framework aliases (`@/`, `~`, custom);
7. unresolved → record `UnresolvedSpecifier`.

Resolution is cached by `(importer, specifier, resolverConfigHash)`.

**Rule C1:** a resolved module must be inside the include set or a declared
external. An external package is not traversed; its exports are assumed live if
imported.

**Rule C2:** a barrel re-export creates an edge to the underlying symbol, not
only to the barrel. This is required for accurate witnesses and cascade closure.

**Rule C3:** an unresolved specifier creates an `unresolved` edge and a
diagnostic. It does not make the target dead. If the specifier is dynamic and
cannot be bounded, create an `unknown` edge with a dynamic scope (see §11).

---

## 8. Phase D — Graph

Build the symbol graph:

- nodes: symbols;
- edges: imports, re-exports, references, calls, JSX, dynamic, unknown, build;
- every edge has a certainty.

Edge construction rules:

| Source form | Edge | Certainty |
|---|---|---|
| `import { x } from './m'` | import x → symbol x in m | `certain` |
| `import * as ns from './m'` | namespace → module | `certain` |
| `ns.x` | reference → x | `certain` if static |
| `export { x } from './m'` | re-export → x | `certain` |
| `export * from './m'` | re-export → all exports of m | `certain`, but each is `probable` if ambiguous |
| `<Button />` | JSX → Button | `certain` |
| `import('./m')` | dynamic → module m | `certain` if literal |
| `require('./m')` | dynamic → module m | `certain` if literal |
| `import(`./locales/${x}.json`)` | dynamic → glob | `probable` |
| `obj[key]` | reference → unknown | `unknown` |
| `import(someVar)` | dynamic → unknown scope | `unknown` |
| `"use server"` export | entry | `certain` |
| build-graph edge | build → target | `certain` |

**Rule D1:** do not prune `unknown` edges for performance. They are correctness
data.

**Rule D2:** do not create an edge from a type-only import to a runtime symbol
unless the compiler preserves it. Type-only edges are marked and do not keep a
runtime symbol live by themselves; they do keep a type symbol live for
type-level analysis.

---

## 9. Phase E — Entries

An entry is a symbol from which reachability starts. Entries are the only reason
a symbol is live.

### 9.1 Next.js App Router

- `page`, `layout`, `template`, `loading`, `error`, `global-error`, `not-found`,
  `route`, `default`;
- `sitemap`, `robots`, `manifest`, `icon`, `apple-icon`, `opengraph-image`,
  `twitter-image`;
- `proxy.ts` (Next.js 16) and `middleware.ts` (15) exports;
- `instrumentation.ts` and `instrumentation-client.ts` `register`;
- `"use server"` module exports (async functions);
- `"use client"` module exports (client boundary);
- metadata exports: `metadata`, `generateMetadata`, `viewport`,
  `generateViewport`, `generateStaticParams`, `generateSitemaps`;
- segment config: `dynamic`, `dynamicParams`, `revalidate`, `fetchCache`,
  `runtime`, `preferredRegion`, `maxDuration`.

### 9.2 Next.js Pages Router

- `pages/**` default exports;
- `pages/api/**` HTTP method exports;
- `_app`, `_document`, `_error`;
- `getServerSideProps`, `getStaticProps`, `getStaticPaths`, `getInitialProps`.

### 9.3 React

- application roots (`createRoot`, `hydrateRoot`);
- route configuration objects;
- lazy imports;
- context providers that are mounted at a root.

### 9.4 Packages

- `package.json` `exports`, `main`, `module`, `types`;
- `index.ts` public surface when the package is a library;
- declaration files are public by default.

### 9.5 Tests and config

- test files, setup files, fixtures;
- `next.config`, `postcss.config`, `tailwind.config`, `eslint.config`,
  `tsconfig`;
- test runner config.

**Rule E1:** an entry is never deleted, regardless of reference count.

**Rule E2:** entry detection is framework-specific and fixture-tested. A generic
"file looks like a page" heuristic is forbidden.

**Rule E3:** test-only and Storybook entries are configurable. Default: on for
local scans, off for production deltas. The receipt records the policy.

---

## 10. Phase F — Reachability with witnesses

### 10.1 Multi-source BFS

```text
queue = all entries, sorted by EntryId
for each entry: mark live, witness[entry] = [entry]
while queue not empty:
    s = pop_front()
    for edge in outgoing(s) sorted by target:
        t = edge.target
        if not live[t]:
            live[t] = true
            parent[t] = (s, edge.id)
            queue.push_back(t)
```

The BFS is over the **over-approximate** graph: `probable` and `unknown` edges
are traversed. This keeps code alive that might be used. It is the conservative
direction.

### 10.2 Witness

For every live symbol, reconstruct the shortest path from its entry by following
`parent` pointers. Store it as a `Vec<(SymbolId, EdgeId)>`.

**Invariant F1:** every live symbol has a witness.

**Invariant F2:** the witness is a valid path in the graph.

**Invariant F3:** the witness is shortest in edge count.

### 10.3 Dead candidates

A symbol is a candidate if it is not live and its file parsed successfully and
it is not an entry and it is not public API.

**Invariant F4:** no entry is a candidate.

**Invariant F5:** no symbol in an unparsed file is a candidate.

### 10.4 Proof trace

For each candidate, record:

- the symbol and span;
- the edges considered (outgoing and incoming);
- the `unknown` edges that could reach it;
- the entry/public-API checks;
- the reason it is not live (`no witness`, `blocked by unknown`, `blocked by
  public API`, `blocked by unparsed file`).

The proof trace is not a formal theorem. It is an auditable record of the
abstraction.

---

## 11. Dynamic-edge abstraction

Dynamic access is the main source of false positives. Shrike handles it in
bounded scopes, from most to least precise:

| Form | Scope | Certainty | Effect |
|---|---|---|---|
| `import('./m')` | literal | `certain` | edge to m |
| `require('./m')` | literal | `certain` | edge to m |
| ``import(`./locales/${x}.json`)`` | static prefix | `probable` | edges to all files under the prefix |
| `import(`./${name}`)` | directory prefix | `probable` | edges to the directory subtree |
| `import(someVar)` in package P | package scope | `unknown` | all files in P are live |
| `import(someVar)` at repo root | repo scope | `unknown` | all files are live; deletion is refused |
| `obj[key]` with literal keys | key set | `probable` | edges to the values |
| `obj[expr]` | module scope | `unknown` | all exports of the module's namespace are live |
| `globalThis[name]` | repo scope | `unknown` | all files are live; deletion is refused |

**Rule G1:** an `unknown` edge is never silently ignored. It is reported in the
proof trace and in `shrike proof`.

**Rule G2:** a `probable` edge blocks deletion unless the candidate is in the
same SCC and has no external live referrer.

**Rule G3:** a dynamic scope is bounded. A repo-scope unknown edge fails closed
rather than pretending to know.

**Rule G4:** a build-graph edge can upgrade an `unknown` to `certain` when the
build proves the target is used.

---

## 12. Phase H — Snapshot

Serialize the graph into a deterministic `GraphSnapshot`:

```text
GraphSnapshot {
  protocolVersion,
  toolVersion,
  configHash,
  resolverHash,
  files: Vec<FileRecord>,       // sorted by path
  symbols: Vec<SymbolRecord>,   // sorted by (file, span.start, kind, name)
  modules: Vec<ModuleRecord>,   // sorted by path
  entries: Vec<EntryRecord>,    // sorted by key
  edges: Vec<Edge>,             // sorted by (source, target, kind, certainty)
  sccs: Vec<Vec<SymbolId>>,     // each sorted; SCCs sorted by min symbol
  live: FixedBitSet,
  candidates: FixedBitSet
}
```

The snapshot hash is BLAKE3 over the canonical serialization. The cache key is
`(hash of file hashes, configHash, resolverHash, toolVersion, protocolVersion)`.

**Invariant H1:** the same input produces the same snapshot bytes.

**Invariant H2:** the snapshot contains no source text beyond spans and names.

**Invariant H3:** the snapshot can be deserialized without re-parsing.

---

## 13. Phase I — Delta

Given base `B` and head `H`:

```text
assert comparable(B, H)
orphaned = live(B) − live(H)
```

The delta is **not** "all dead code at head." It is the **newly** dead code
caused by the change. Pre-existing debt is out of scope unless the user runs a
full scan.

**Comparability** requires:

- same config hash;
- same resolver hash;
- same entry adapter version;
- same tool and protocol version;
- same workspace layout.

If any differs, the delta is `incomparable` and the tool explains which input
changed. It does not silently produce a misleading diff.

**Invariant I1:** a no-op PR produces an empty delta.

**Invariant I2:** pre-existing dead code is not in the delta.

**Invariant I3:** a symbol that was already dead at base is not reported as newly
dead.

---

## 14. Phase J — Trigger attribution

For each orphaned symbol:

1. Get its witness at base.
2. Diff the base and head edge sets.
3. Walk the witness from the entry to the symbol; find the first edge that is
   missing or changed at head.
4. Attribute the orphan to the changed file/symbol that owns that edge.
5. If the edge is still present but the target changed, attribute to the target
   change.
6. If no single edge explains it, attribute to the closest changed ancestor on
   the witness and mark `indirect`.

Output:

```json
{
  "symbol": "src/lib/format.ts#formatInvoice",
  "lastWitness": [
    "app/invoices/page.tsx#default",
    "src/features/invoice/InvoiceCard.tsx#InvoiceCard",
    "src/lib/format.ts#formatInvoice"
  ],
  "trigger": {
    "file": "src/features/invoice/InvoiceCard.tsx",
    "change": "removed import of formatInvoice",
    "edge": "InvoiceCard → formatInvoice",
    "certainty": "certain"
  }
}
```

**Invariant J1:** every delta item has a lost witness or an explicit
`no-witness` reason.

**Invariant J2:** the trigger is a changed edge, not a guess about intent.

---

## 15. Phase K — SCC condensation

Run Tarjan's algorithm over the symbol graph. Treat each SCC as a unit.

Why SCC first:

- A cycle is a unit. Deleting one node of a cycle can leave the rest live and
  produce an unstable plan.
- The cascade closure is simpler and safer on the condensation DAG.
- Batch partitioning operates on SCCs, so no batch can split a cycle.

Rules:

- An SCC is **live** if any of its symbols is live.
- An SCC is **dead** if all its symbols are candidates and no external live edge
  enters it.
- An SCC with an entry role or public API marker is live.
- An SCC with an incoming `unknown` edge is live (blocked).

**Invariant K1:** no deletion plan contains a partial SCC.

**Invariant K2:** SCC IDs are deterministic (sorted by minimum `SymbolId`).

Implementation: iterative Tarjan to avoid stack overflow on deep graphs.

---

## 16. Phase L — Cascade closure

Build the condensation DAG `D` (SCCs as nodes, symbol edges collapsed).

Given dead seed SCCs `S`:

```text
closure = {}
queue = S
while queue not empty:
    scc = pop(queue)
    if scc in closure: continue
    if hasEntryRole(scc) or isPublicApi(scc) or hasUnknownIncoming(scc): continue
    if hasExternalLivePredecessor(scc, closure): continue
    closure.add(scc)
    for succ in successors(scc):
        if succ not in closure and allLivePredecessorsInClosure(succ, closure):
            queue.push(succ)
```

Stop conditions:

- entry role;
- public API marker (`package.json` exports, `index.ts` surface, declaration
  file, explicit `@public`);
- external live referrer;
- incoming `unknown` edge;
- `probable` edge outside the SCC;
- unparsed file;
- generated file (excluded by policy);
- test-only entry (when the policy excludes tests from deletion);
- blast-radius policy (a closure that exceeds the cap is split or refused).

**Invariant L1:** the closure contains complete SCCs only.

**Invariant L2:** every symbol in the closure has all live referrers inside the
closure.

**Invariant L3:** the closure is deterministic.

---

## 17. Phase M — Impact estimation

For each closure:

- `symbols`: count;
- `filesTouched`: unique files;
- `filesRemoved`: files where every symbol is deleted and no live content
  remains;
- `linesRemoved`: union of symbol spans per file, merged to avoid double
  counting;
- `routesAffected`: from base witnesses and build evidence;
- `bytes`: from build evidence only, with shared chunks attributed once;
- `packagesAffected`: workspace packages.

**Rule M1:** never report a single "bytes saved" number when chunks are shared.
Report per-route and total separately.

**Rule M2:** line counts are estimates; the receipt records the exact patch
stats after apply.

---

## 18. Phase N — Batch partitioning

### 18.1 Conflict graph

Nodes are deletion units (SCCs). Two units conflict if:

- they touch the same file;
- one is a predecessor of the other in the condensation DAG;
- one is a re-export barrel that changes the other's import path;
- they share a generated or public API boundary.

### 18.2 Greedy coloring

```text
units = sorted(closure, by impact desc, then min SymbolId)
for unit in units:
    for color in colors:
        if no conflict with any unit in color:
            add unit to color
            break
    else:
        new color
```

Each color is a batch. A batch has no internal file conflicts, so its patch is
independent.

### 18.3 Batch cap

- default: ≤ 50 symbols, ≤ 10 files, ≤ 5% of the live graph;
- configurable in `shrike.config.ts`;
- if a color exceeds the cap, split it by file groups, then by sorted unit IDs;
- if a single SCC exceeds the cap, the batch is `oversized` and requires
  `--allow-oversized`; the receipt records the override.

**Invariant N1:** no batch shares a file with another batch.

**Invariant N2:** no batch splits an SCC.

**Invariant N3:** batch order is deterministic.

**Invariant N4:** no batch exceeds the blast-radius policy unless explicitly
overridden.

### 18.4 Why greedy coloring

The conflict graph is an interval-like graph in practice (files and dependency
order), so greedy coloring is near-optimal and deterministic. Exact coloring is
NP-hard and unnecessary. The batch cap bounds the bisection cost.

---

## 19. Phase O — Blast-radius guard

The guard is a product feature, not an optimization.

A batch is refused or split when:

- it touches more than `maxFiles`;
- it deletes more than `maxSymbols`;
- it removes more than `maxLiveGraphPercent` of the live graph;
- it touches a public API surface;
- it touches a file with an `unknown` edge;
- it crosses a workspace boundary without an explicit allow.

The output is a `BlastRadiusReport`:

```json
{
  "batch": "batch_1",
  "symbols": 52,
  "files": 14,
  "liveGraphPercent": 0.8,
  "policy": { "maxSymbols": 50, "maxFiles": 10, "maxLiveGraphPercent": 5 },
  "decision": "split",
  "splitInto": ["batch_1a", "batch_1b"]
}
```

**Rule O1:** a refused batch is reported, not silently dropped.

**Rule O2:** an override is explicit, recorded, and visible in the PR.

---

## 20. Phase P — Patch materialization

For each batch:

1. Compute the deletion edit set: remove declarations, exports, imports that
   become unused, and statements that are exclusively owned.
2. Preserve side-effect imports (`import './polyfill'`) unless the module is
   proven side-effect-free and the policy allows it.
3. Do not delete comments that explain a live symbol; delete comments attached to
   a deleted symbol only when they are exclusively owned.
4. Produce a unified diff per file.
5. Compute patch stats: files, insertions, deletions.
6. Store the patch under `.shrike/patches/<batch>.patch`.

**Rule P1:** the edit set is derived from the plan, not from a text search.

**Rule P2:** a patch that touches a file outside the plan is a bug and fails
closed.

**Rule P3:** a patch that changes runtime behavior beyond deletion is out of
scope; Shrike deletes, it does not refactor.

---

## 21. Phase Q — Verification (host)

The host executes the contract in
[05-verified-deletion.md](05-verified-deletion.md):

```text
baseline → apply batch → checks → compare → keep or revert
```

The engine does not run checks. The host does.

---

## 22. Phase R — Bisection

When a batch fails:

1. Split the batch into halves by sorted SCC units, respecting the no-shared-file
   invariant.
2. Verify each half against the same baseline.
3. If one half fails, recurse into it.
4. If both halves pass but the full batch fails, record an `interaction-failure`
   and revert the whole batch.
5. Stop at a single SCC.
6. Revert the minimal failing set; record it with the check and signature.
7. Retry the remainder, excluding the culprit.

**Rule R1:** bisection is deterministic. Sort by `SymbolId` before splitting.

**Rule R2:** the same failure produces the same rejection set.

**Rule R3:** a failed bisection (e.g., timeout) reverts the whole batch and exits
4; it does not guess.

**Rule R4:** an interaction failure is not attributed to a single symbol. The
receipt says so.

---

## 23. Phase S — Receipt

The receipt records:

- base and head commits and graph hashes;
- plan and batch hashes;
- symbols, files, lines, patches;
- each check: command, status, baseline status, duration, reruns;
- outcome: `verified`, `rejected`, `known-failure`, `flaky`, `error`,
  `revert-failed`, `interaction-failure`;
- rejected sets with check name and failure signature;
- build evidence status and hash;
- environment;
- policy (entry policy, batch cap, blast radius).

The receipt ID is a content hash of the inputs and outputs, excluding
`createdAt`.

Full schema: [`../schemas/receipt.schema.json`](../schemas/receipt.schema.json).

---

## 24. Determinism

Determinism is enforced at every phase:

- files sorted by normalized path;
- symbols sorted by `(file, span.start, kind, name)`;
- edges sorted by `(source, target, kind, certainty)`;
- entries sorted by key;
- BFS adjacency sorted by target;
- SCCs sorted by minimum symbol;
- batches sorted by impact then minimum symbol;
- parallel parse results merged in file order;
- no hash-map iteration in output;
- no timestamps in IDs;
- BLAKE3 for content hashes.

Tests:

- run the same scan twice, compare snapshot bytes;
- run with 1, 2, 8 threads, compare output;
- run cold and warm, compare output;
- run with files in a different filesystem order, compare output.

---

## 25. Complexity and performance

| Phase | Complexity | Notes |
|---|---|---|
| Parse | O(total source bytes) | parallel by file |
| Resolve | O(specifiers × cache) | parallel by importer |
| Graph build | O(symbols + edges) | sequential, CSR |
| Reachability | O(symbols + edges) | BFS |
| SCC | O(symbols + edges) | iterative Tarjan |
| Cascade | O(symbols + edges) | condensation DAG |
| Delta | O(symbols + edges) | bitset difference |
| Conflict graph | O(units²) worst case | units are SCCs; typically small |
| Coloring | O(units × colors) | greedy |
| Bisection | O(log batch × check cost) | check cost dominates |

Memory:

- per-file summary, not AST;
- interned strings;
- CSR edges;
- bitsets for live/candidate;
- snapshot on disk for base/head reuse.

**Rule 25.1:** measure before optimizing.

**Rule 25.2:** the 10,000-file budget is 600 MB with two snapshots. If the
implementation exceeds it, reduce retention before changing the target.

---

## 26. Why this algorithm is better

| Competitor pattern | Shrike difference |
|---|---|
| Current-state dead-code scan | Base/head delta reports what this PR killed, not just what is dead |
| Changed-file audit | Whole-graph reachability finds orphans in untouched files |
| Per-symbol deletion | SCC-first closure and independent batches avoid partial cycles and dangling imports |
| Advisory verdict | Verified batch PR with baseline, after-checks, bisection, and receipt |
| Unbounded fix | Blast-radius guard and per-batch commits keep review tractable |
| Best-effort confidence | Witnesses, proof traces, and `unknown` blocking make every claim explainable |
| Text/regex heuristics | `oxc` AST and semantic data, with explicit certainty |
| Non-deterministic output | Stable IDs, sorted arenas, byte-identical snapshots |

---

## 27. Invariants (summary)

| ID | Invariant |
|---|---|
| A1 | File list is sorted and unique |
| B1 | Unparsed files never yield candidates |
| C1 | Unresolved specifiers are recorded, not dropped |
| D1 | Unknown edges are never pruned |
| E1 | Entries are never deleted |
| F1 | Every live symbol has a witness |
| F2 | Every witness is a valid path |
| F3 | Every witness is shortest in edge count |
| G1 | Unknown edges are visible in the proof trace |
| H1 | Same input → same snapshot bytes |
| I1 | No-op PR → empty delta |
| I2 | Pre-existing dead code is not in the delta |
| J1 | Every delta item has a lost witness or explicit reason |
| K1 | No partial SCC is deleted |
| L1 | The closure contains complete SCCs only |
| L2 | Every live referrer of a closure symbol is inside the closure |
| N1 | No batch shares a file with another batch |
| N2 | No batch splits an SCC |
| N3 | Batch order is deterministic |
| N4 | Blast radius is bounded unless explicitly overridden |
| P2 | A patch never touches a file outside the plan |
| R1 | Bisection is deterministic |
| S1 | The receipt is complete or the batch is not `verified` |

---

## 28. Open questions

| ID | Question | Owner |
|---|---|---|
| Q-010 | Monorepo workspace representation | PLAN-2 |
| Q-012 | Test/storybook entries by default | PLAN-2 |
| Q-013 | Public API outside `package.json` exports | PLAN-2 |
| Q-025 | Feature removal vs rename | PLAN-2 |
| Q-029 | Blast-radius threshold configuration | PLAN-2 |
| Q-035 | Should type-only edges keep runtime symbols live? | PLAN-1 |
| Q-036 | How are ambient/global declarations modeled? | PLAN-1 |
| Q-037 | How are generated files detected across frameworks? | PLAN-2 |
