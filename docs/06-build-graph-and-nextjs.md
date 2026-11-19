# 06 — Build Graph and Next.js

> **Parent:** [MASTER_PROMPT.md](../MASTER_PROMPT.md) · **Tracker:** [TRACKER.md](../TRACKER.md)
> **Status:** canonical · Authored 2026-11-20

---

## 1. Why build-graph evidence

Static reachability answers a source-level question: "is there a path from an
entry?" It cannot answer the shipping question: "is this module in the client
bundle, on which routes, and how many bytes does it cost?"

That matters for three reasons:

1. **Confirmation.** A build graph that shows a module is absent from every
   client bundle is stronger evidence than a static graph alone.
2. **Attribution.** "This dead subtree costs 41 KB across 6 routes" is a
   decision-grade number; "18 unused exports" is not.
3. **Soundness.** A build graph can reveal a dynamic edge the static graph
   missed. That suppresses a false positive instead of causing one.

The build graph is **optional evidence**, not a hard dependency. If the snapshot
is absent, stale, or from an unsupported version, Shrike runs static-only and
records the reason. It never silently pretends the build evidence was used.

---

## 2. The Next.js surface

Shrike targets Next.js 16.x, with a documented fallback matrix for 15.x.

### 2.1 `next analyze export`

Next.js 16.4 ships `next analyze export`, which writes the Turbopack module and
chunk graph as JSON Lines. The exact record shapes are versioned; Shrike treats
them as an external contract with additive tolerance.

Expected record families:

| Record | Contains | Used for |
|---|---|---|
| `module` | module id, source path, layer (client/server), size | module index, byte attribution |
| `chunk` | chunk id, route or entry, module ids, size | route membership, shared-chunk attribution |
| `entry` | route, runtime, chunk ids | route mapping |
| `asset` | emitted file, size | sanity check, not required |
| `client-reference` | server→client boundary mapping | server action and client boundary analysis |

The parser is streaming: one JSON object per line, no full-file load, no
assumption about record order, and additive fields are ignored rather than
rejected.

### 2.2 Version guard

| Next.js | Behavior |
|---|---|
| 16.4+ | Full evidence: presence, routes, bytes, client references |
| 16.1–16.3 | Partial: presence and routes if the export exists; bytes marked partial |
| 15.x | Static only; a warning explains the minimum version |
| < 15 | Unsupported; static only |

The guard reads `next` from the project's `package.json` and the export command's
own output. If they disagree, the lower capability wins.

### 2.3 What Shrike does not do

- It does not run `next build` unless `--build` is explicit.
- It does not modify `next.config`.
- It does not suggest code splitting, lazy loading, RSC migration, or dependency
  deduplication.
- It does not treat the build graph as source proof.

---

## 3. Ingestion pipeline

```text
snapshot file (JSONL)
  → stream parse
  → normalize paths and module ids
  → module index (path → module, id → module)
  → chunk index (chunk → modules, route → chunks)
  → client-reference index
  → join with the static graph
  → evidence verdict per candidate
  → byte attribution
  → receipt record
```

### 3.1 Path normalization

Turbopack module paths may be:

- absolute paths;
- repo-relative paths;
- `turbopack://` virtual paths;
- loader-prefixed paths;
- query-suffixed paths.

Normalization:

1. strip a loader prefix (`next/dist/...!`);
2. strip a query string;
3. resolve to the repository root;
4. convert to `/` separators;
5. match the static file table by exact path, then by suffix (longest match);
6. record unmatched modules.

Unmatched modules are **unknown**, never guessed.

### 3.2 Module index

```text
ModuleIndex {
  byPath: Map<normalizedPath, ModuleId>,
  byId: Map<moduleId, ModuleId>,
  modules: Vec<ModuleRecord { path, layer, size, chunks }>,
  unmatched: Vec<UnmatchedModule>
}
```

The index is built once per snapshot and cached by `(snapshotHash, nextVersion)`.

### 3.3 Join coverage

The join reports coverage:

```text
joinCoverage = matchedModules / (matchedModules + unmatchedModules)
```

- `≥ 0.95`: full evidence;
- `0.80–0.95`: partial evidence, with an explicit status;
- `< 0.80`: evidence is not used for confirmation; it may still suppress.

A low join coverage is a product signal (a Next.js change, a loader pattern, or a
bug), not something to hide.

---

## 4. Evidence verdicts

For each deletion candidate:

| Build result | Verdict | Effect |
|---|---|---|
| Present in a client chunk on a live route | `present` | **Suppress** deletion |
| Present in a server chunk only | `server-only` | Weaken; deletion still needs source proof and a server check |
| Absent from every chunk, joined | `absent` | **Confirm** |
| Module unjoined | `unknown` | Do not confirm; report why |
| Version unsupported | `unsupported` | Static-only; report |
| Snapshot stale | `stale` | Static-only; report |

"Stale" means the snapshot's source hash does not match the current tree's hash
for the files it references, or its build timestamp predates the head commit.

### 4.1 Suppression is mandatory

A `present` verdict blocks deletion regardless of the static graph. The build
proves the module ships; a static graph that says otherwise is wrong somewhere,
and that is a bug to investigate, not an invitation to delete.

### 4.2 Confirmation is advisory

An `absent` verdict does not permit deletion by itself. It adds evidence to a
static candidate. The verifier still decides.

---

## 5. Dynamic-edge feedback

The build graph can reveal a dynamic edge that the static graph missed:

- a runtime registry resolves a component by key;
- a plugin directory is loaded by a generated manifest;
- a route is created by a convention the adapter does not know.

When the build shows module `M` is used, Shrike adds a `build` edge from the
nearest static importer (or from the build entry) to `M` with certainty
`certain`, and re-runs reachability. The candidate is suppressed.

The engine records:

```json
{
  "kind": "build-edge",
  "from": "app/registry.ts#registry",
  "to": "src/plugins/chart.tsx#ChartPlugin",
  "evidence": "chunk:app/registry/page",
  "certainty": "certain"
}
```

**Rule:** a build edge can only add liveness, never remove it. The build graph is
not used to prove death; it is used to avoid false positives and to attribute
impact.

---

## 6. Route attribution

Chunks are attributed to routes:

- a route chunk belongs to its route;
- a shared chunk belongs to every route that loads it;
- a framework chunk is not attributed to a user route;
- a client chunk and a server chunk are recorded separately.

Output:

```json
{
  "symbol": "src/lib/format.ts#formatInvoice",
  "routes": [
    { "route": "/invoices", "runtime": "client", "bytes": 1820 },
    { "route": "/invoices/[id]", "runtime": "client", "bytes": 1820 }
  ],
  "sharedBytes": 1820,
  "totalRouteBytes": 3640
}
```

### 6.1 Byte attribution rules

- Report **per route** and **shared** separately.
- Never sum shared bytes once per route and call it "total saved"; that double
  counts.
- `sharedBytes` is the size of a module present in a shared chunk.
- `totalRouteBytes` is the sum of per-route first-load bytes that include the
  module, excluding shared chunks already counted.
- The receipt records the snapshot hash and the attribution method.

If exact attribution is impossible (e.g., minified chunk composition), report
`partial` and the reason. Do not fabricate precision.

---

## 7. Server actions and client boundaries

### 7.1 Server actions

A `"use server"` module export is an entry. Its reachability is from the client
reference that invokes it.

- If the client-reference manifest maps the action to a client module, create a
  `certain` edge.
- If the manifest is unavailable, keep the action live by default. A server
  action is never a deletion candidate without positive evidence that no client
  reference exists.
- Build evidence that an action is absent from every client reference is
  `absent`, not proof of death; the action may be called from a form, a fetch, or
  an external client.

### 7.2 Client boundaries

- A `"use client"` module is an entry only when imported from a server
  component, a route, or another entry.
- A `"use client"` module that is never imported is a candidate, but it is
  blocked if it is also a route convention file.
- The build snapshot can confirm the boundary by layer. A mismatch between the
  directive and the layer is a diagnostic, not a finding.

### 7.3 Leak diagnostic

If a server-only module appears in a client chunk, Shrike reports a
`client-boundary-leak` diagnostic. It is not a dead-code finding, but it is
useful and cheap to detect from the same snapshot. It does not block the run.

---

## 8. Proxy, middleware, metadata

These are entries and are never deletion candidates:

- `proxy.ts` / `proxy.js` (Next.js 16) and `middleware.ts` / `middleware.js`;
- `instrumentation.ts`, `instrumentation-client.ts`;
- `app/**/sitemap.ts`, `robots.ts`, `manifest.ts`;
- `app/**/icon.*`, `apple-icon.*`, `opengraph-image.*`, `twitter-image.*`;
- `app/**/not-found.tsx`, `error.tsx`, `global-error.tsx`, `loading.tsx`,
  `template.tsx`;
- metadata exports: `metadata`, `generateMetadata`, `viewport`,
  `generateViewport`, `generateStaticParams`, `generateSitemaps`.

The matcher config in `proxy.ts` / `middleware.ts` is parsed but not evaluated;
the export itself is the entry. A matcher that excludes everything is still an
entry, because it can be changed without touching the file.

---

## 9. Fallback matrix

| Condition | Status | Behavior |
|---|---|---|
| Snapshot exists, version 16.4+, coverage ≥ 0.95 | `full` | Confirm, suppress, attribute bytes |
| Snapshot exists, version 16.1–16.3 | `partial` | Presence and routes; bytes marked partial |
| Snapshot exists, coverage 0.80–0.95 | `partial` | Confirm only with source proof; suppress on presence |
| Snapshot exists, coverage < 0.80 | `low-coverage` | Suppress only; do not confirm |
| Snapshot missing and `--build` absent | `absent` | Static-only; explain how to produce one |
| Snapshot missing and `--build` present | `build-failed` | Static-only; record the build error; fail the receipt if the user required evidence |
| Snapshot stale | `stale` | Static-only; explain the mismatch |
| Snapshot unparseable | `malformed` | Static-only; record the line and the error |
| Next.js < 16.1 | `unsupported` | Static-only; no warning spam |

`--require-build-evidence` turns `absent`, `stale`, `malformed`, and
`unsupported` into exit 4. The default is static-only with a recorded status.

---

## 10. Data model

```text
BuildSnapshot {
  schemaVersion: String,
  nextVersion: String,
  createdAt: String,
  sourceHash: String,
  modules: Vec<BuildModule>,
  chunks: Vec<BuildChunk>,
  routes: Vec<BuildRoute>,
  clientReferences: Vec<ClientReference>,
  unmatched: Vec<UnmatchedModule>,
  status: full | partial | low-coverage | stale | malformed | unsupported
}

BuildModule {
  id: String,
  path: Option<String>,
  layer: client | server | edge | unknown,
  size: Option<u64>,
  chunks: Vec<ChunkId>
}

BuildChunk {
  id: String,
  kind: app | pages | framework | shared | unknown,
  route: Option<String>,
  modules: Vec<ModuleId>,
  size: Option<u64>
}

BuildRoute {
  route: String,
  runtime: client | server | both,
  chunks: Vec<ChunkId>
}

ClientReference {
  from: String,
  to: String,
  kind: server-action | client-component | unknown
}
```

Full schema: [`../schemas/graph-snapshot.schema.json`](../schemas/graph-snapshot.schema.json)
(the evidence section) and [`../schemas/receipt.schema.json`](../schemas/receipt.schema.json)
(the build record).

---

## 11. Fixtures

| Fixture | What it proves |
|---|---|
| `build-graph/next-16.4/` | Full snapshot parses; presence, absence, routes, bytes |
| `build-graph/next-16.1/` | Partial snapshot; bytes marked partial |
| `build-graph/next-15/` | Unsupported version; static-only |
| `build-graph/missing/` | Missing snapshot; static-only with a clear status |
| `build-graph/stale/` | Source hash mismatch; stale |
| `build-graph/malformed/` | Invalid JSONL; parse error recorded; static-only |
| `build-graph/low-coverage/` | Unjoined modules; no confirmation |
| `build-graph/dynamic-edge/` | Build edge suppresses a false positive |
| `build-graph/shared-chunk/` | Shared bytes are not double counted |
| `build-graph/server-action/` | Server action is never confirmed dead from absence |
| `build-graph/client-boundary/` | Layer mismatch diagnostic |

Each fixture contains a real `next analyze export` output (trimmed), the static
graph, and the expected evidence join.

---

## 12. Acceptance criteria

- [ ] `next analyze export` JSONL streams without loading the whole file.
- [ ] Module join coverage is reported and ≥ 0.90 on the reference app.
- [ ] A module present in a client chunk suppresses deletion.
- [ ] A module absent from every chunk confirms, but never authorizes, deletion.
- [ ] Bytes are reported per route and shared, with no double counting.
- [ ] A build edge suppresses a false positive and is recorded.
- [ ] Version guard degrades cleanly for 16.1–16.3 and 15.x.
- [ ] Missing, stale, malformed, and low-coverage snapshots are statuses, not
      crashes.
- [ ] `--require-build-evidence` exits 4 when evidence is unavailable.
- [ ] Server actions and client boundaries are conservative.
- [ ] The must-not-report corpus is green with and without build evidence.
- [ ] The receipt records the build status, version, and hash.

---

## 13. Open questions

| ID | Question | Owner |
|---|---|---|
| Q-008 | Minimum Next.js version for build-graph evidence? | PLAN-4: 16.1 partial, 16.4 full |
| Q-014 | Is the `next analyze export` schema documented per version? | PLAN-4 |
| Q-015 | How are Turbopack virtual modules represented? | PLAN-4 |
| Q-016 | Does Shrike run `next build` itself? | PLAN-4: only with `--build` |
| Q-027 | Is the client-reference manifest available across versions? | PLAN-4 |
| Q-042 | How are monorepo builds with multiple Next.js apps handled? | PLAN-4 |
| Q-043 | How are route groups and parallel routes attributed? | PLAN-4 |
