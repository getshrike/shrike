# 06 — Build Graph and Next.js

> **Parent:** [MASTER_PROMPT.md](../MASTER_PROMPT.md) · **Tracker:** [TRACKER.md](../TRACKER.md)
> **Status:** canonical · Authored 2026-11-20
> **Implementation status (2026-11-20): implemented for the master-prompt contract; local acceptance is green.** `shrike evidence` discovers `.next/analyze/*.json[l]`, `.shrike/build/*.json[l]`, or `next-analyze/*.json[l]`, or reads an explicit `--build <path>`. JSONL is streamed in bounded chunks; one JSON object is also accepted. The parser understands the real Next.js 16.4 Turbopack families (`meta`, `module`, `route`, `output`, `part`, `group`) as well as the older synthetic `chunk`/`entry` shapes. Malformed input fails closed with exit 4, and unknown record families are counted rather than guessed. The join reports coverage (`full` ≥ 0.95, `partial` ≥ 0.80, `low-coverage` < 0.80, `unsupported` below Next.js 16.1, `none` when absent), per-route membership, matched/shared/route-local bytes, missing-size counts (`bytes.unknown`, `bytes.partial`, `unknownSizes`), per-candidate `present`/`server-only`/`absent`/`unknown` verdicts, build edges from client-reference records, and a `client-boundary-leak` warning. `[project]/`, `[externals]/`, `[client-fs]/`, `[output]/`, `[next]/`, and `[turbopack]/` paths, Turbopack layer annotations, proxy modules, loader prefixes, query strings, `turbopack://` paths, module IDs, and longest-suffix matches are normalized by the host and engine; framework, dependency, and emitted-asset modules are excluded from join coverage so a real app can reach `full`. Source hashes in `fileHashes` mark stale evidence; `--run-build` records `build-failed` instead of crashing; `--require-build-evidence` turns unavailable, stale, unsupported, or low-confidence evidence into exit 4. `augment-build` seeds build-live modules and symbols and follows known static edges, so build evidence can only add liveness. `prune --build` records source, snapshot hash, version, and status in every receipt. The eleven `fixtures/build-graph/` scenarios, the host Turbopack tests, and the corpus evidence mode run in `node scripts/run-fixtures.mjs`. A local real Next.js 16.4.2 reference app joins at `full` coverage (25/25 project modules, 46,956 matched bytes, 5 routes); external partner-app runs remain under the distribution gate.

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

Expected record families (Next.js 16.4):

| Record | Contains | Used for |
|---|---|---|
| `meta` | schema version, snapshot name, route counts | provenance; no join evidence |
| `module` | `ident`, `path`, dependency lists, layer annotation | module index, layers, module IDs |
| `route` | route name, entry module idents/paths | route mapping |
| `output` | route, emitted filename, module idents, coverage | route membership and client/server runtime |
| `part` | route, emitted filename, `source_path`, `size`, `compressed_size` | byte attribution |
| `group` | route, bootstrap group, emitted chunk filenames | understood; no source modules |
| `chunk` / `entry` | older synthetic route/chunk shapes | route membership, shared-chunk attribution |
| `asset` | emitted file, size | sanity check, not required |
| `client-reference` | server→client boundary mapping | server action and client boundary analysis |

The parser is streaming: one JSON object per line, no full-file load, no
assumption about record order, and additive fields are ignored rather than
rejected. Real exports include framework modules, `node_modules` dependencies,
`[externals]` shims, and emitted assets; those are classified and excluded from
the project-module join rather than counted as unmatched.

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
- `[project]/` project modules;
- `[externals]/`, `[next]/`, `[turbopack]/`, `[client-fs]/`, and `[output]/`
  framework, external, and emitted-asset paths;
- `turbopack://` virtual paths;
- loader-prefixed paths;
- query-suffixed paths;
- `[layer] (kind)` annotated module identifiers;
- `__nextjs-internal-proxy.mjs` modules that name the real source in a
  `{ server => "..." }` annotation.

Normalization:

1. unwrap a Turbopack proxy module to its named source;
2. strip a loader prefix (`next/dist/...!`);
3. strip a query string and a `[layer] (kind)` annotation;
4. strip the `[project]/` prefix; classify `[externals]/`, `[next]/`,
   `[turbopack]/`, `[client-fs]/`, and `[output]/` as non-project;
5. resolve to the repository root;
6. convert to `/` separators;
7. match the static file table by exact path, then by suffix (longest match);
8. record unmatched **project source** modules.

Dependency modules (`node_modules/`), framework modules, generated wrappers
(`--route-entry`, `.[json]`, `.next-internal/`), and emitted assets are not
project candidates and are excluded from join coverage. Unmatched project
modules are **unknown**, never guessed.

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
joinCoverage = matchedProjectModules / (matchedProjectModules + unmatchedProjectModules)
```

Only project source modules participate. `node_modules` dependencies,
`[externals]` shims, framework runtime modules, generated wrappers, and emitted
assets are classified and excluded; counting them would make every real
application look like 2% coverage. An unmatched project module still counts as
unknown and lowers coverage.

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

A `"use server"` module export is a server action. Under the default
`serverActionPolicy: "graph"`, it is an ordinary export: an import, a
`action={fn}` binding, a call, or any other project reference keeps it live,
and an action with no project reference is reported. This matches knip and
fallow. `serverActionPolicy: "protect"` instead seeds every export and the
module as entries, because Next.js registers actions in the server-reference
manifest once the module is reached.

- If the client-reference manifest maps the action to a client module, create a
  `certain` edge. This keeps a referenced action live under either policy.
- If the manifest is unavailable, the default policy relies on static
  references. An unreferenced action remains POST-able if its id leaks, which
  is why `protect` exists for teams that treat that as a supported entry point.
- Build evidence that an action is absent from every client reference is
  `absent`. Under `protect` it is not proof of death; under `graph` it
  corroborates the static finding and must not resurrect a dead action.

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

### 7.4 Invalid client exports

Next.js rejects a `"use client"` module that exports a server-only,
route-config, or route-handler name. Shrike reports an advisory
`invalid_client_export` warning for the 22 names fallow 3.45.0 lists:
`metadata`, `generateMetadata`, `viewport`, `generateViewport`,
`generateStaticParams`, `dynamic`, `dynamicParams`, `revalidate`,
`fetchCache`, `runtime`, `preferredRegion`, `maxDuration`,
`getServerSideProps`, `getStaticProps`, `getStaticPaths`, and the route
handlers `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD`, `OPTIONS`.

The rule is project-wide: it fires when the root manifest or any workspace
package declares `next`. A shared `"use client"` module is compiled in the
consuming Next app's context, so its own package need not declare Next. A
tree without any Next dependency produces no warnings.

Direct exports are anchored on the declaration. A local
`const metadata; export { metadata }` is anchored on the export specifier and
reported once, so the fix points at the line a developer must change. Default
exports, `export type`, and `export { metadata } from "./server"` re-exports
are not flagged. The warning is advisory: it never removes a candidate or
suppresses an entry.

### 7.5 Route conflicts

Two App Router files can make Next.js reject or crash a route without any dead
code. Shrike reports both as error diagnostics, never as findings:

- `route_collision`: two `page`/`route` leaves in one app root resolve to the
  same URL. Next.js fails the build with "You cannot have two parallel pages
  that resolve to the same path". A page and a route handler share the URL
  namespace.
- `dynamic_segment_name_conflict`: sibling dynamic segments at one tree
  position use different spellings (`[id]` vs `[slug]`). `next build` succeeds,
  but the route throws the first time it is resolved.

The URL is a pure function of the path: route groups `(x)` are stripped,
parallel `@slot` segments fork the namespace (two slots may own the same URL),
and private `_folder` plus intercepting `(.)`/`(..)`/`(...)` subtrees are not
routable participants. The app root is `<package>/app` or `<package>/src/app`;
a bare `app` directory is never an app root. Both checks require the owning
package to declare `next`, so a React-only workspace package with an
app-shaped directory is left alone. This is deliberately more precise than
fallow 3.45.0's project-wide gate, which flags that package when a sibling
declares Next.

### 7.6 Misplaced directives

React Server Components bundlers honor `"use client"` and `"use server"` only
in the leading prologue, before any other statement. A directive-looking
string after an import or a declaration is parsed as an ordinary expression
statement and silently ignored: the developer intended a boundary, but the
module is treated as a server module. Shrike reports a `misplaced_directive`
warning anchored on the offending statement. It is not a finding and never
changes candidates.

The rule is project-wide and gated on a React Server Components bundler:
`next`, `waku`, `@lazarv/react-server`, `react-server-dom-webpack`,
`react-server-dom-vite`, `react-server-dom-parcel`, or `@vitejs/plugin-rsc`
in the root or any workspace manifest. A leading directive is honored even
when imports follow it; a nested string and a stray `"use strict"` are not
directives. One warning is emitted per misplaced occurrence. The predicate and
the two recognized strings match fallow 3.45.0's `misplaced_directive`
detector.

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
| Snapshot exists, version 16.1–16.3 | `partial` | Presence and routes; `bytes.partial` is set |
| Snapshot exists, coverage 0.80–0.95 | `partial` | Confirm only with source proof; suppress on presence |
| Snapshot exists, coverage < 0.80 | `low-coverage` | Suppress only; do not confirm |
| Snapshot missing and no evidence path | `none` | Static-only; receipts record `absent`; explain how to produce one |
| `--run-build` command fails | `build-failed` | Static-only; record the error; fail closed only when evidence is required |
| Explicit `--build` path missing | error | Exit 4; no partial evidence is invented |
| Snapshot stale | `stale` | Static-only; explain the source-hash mismatch |
| Snapshot unparseable | `malformed` | Static-only; record the line and the error |
| Next.js < 16.1 | `unsupported` | Static-only; no warning spam |

`--require-build-evidence` turns `none`, `stale`, `malformed`, `unsupported`,
`low-coverage`, and `build-failed` into exit 4. The default is static-only with a
recorded status.

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

Each scenario contains a trimmed `next analyze export` output and expected
evidence join. The scenarios share `fixtures/build-graph/source/` as their static
graph; `scripts/run-fixtures.mjs` analyzes it once and checks every join,
augmentation, and host-level fallback (`missing`, `stale`, `malformed`). The
`build-graph/next-16.4/` scenario is the full reference join; `shared-chunk/`
proves byte arithmetic; `dynamic-edge/` and `server-action/` prove build-edge
liveness; `client-boundary/` proves the warning diagnostic. The host parser is
covered separately by `packages/cli/test/build-evidence.test.mjs`, which feeds
real Turbopack `meta`/`module`/`route`/`output`/`part`/`group` records through
`shrike evidence` and asserts full coverage, project-only files, layers, routes,
and byte totals.

---

## 12. Acceptance criteria

- [x] `next analyze export` JSONL streams without loading the whole file.
- [x] Module join coverage is reported and ≥ 0.90 on the reference app.
      Local Next.js 16.4.2 reference app: 25/25 project modules, coverage 1.00,
      46,956 matched bytes across 5 routes (`shrike evidence --build`). External
      partner-app runs remain under the distribution gate.
- [x] A module present in a client chunk suppresses deletion.
- [x] A module absent from every chunk confirms, but never authorizes, deletion.
- [x] Bytes are reported per route and shared, with no double counting.
- [x] A build edge suppresses a false positive and is recorded.
- [x] Version guard degrades cleanly for 16.1–16.3 and 15.x.
- [x] Missing, stale, malformed, and low-coverage snapshots are statuses, not
      crashes.
- [x] `--require-build-evidence` exits 4 when evidence is unavailable.
- [x] Server actions and client boundaries are conservative.
- [x] The must-not-report corpus is green with and without build evidence.
- [x] The receipt records the build status, version, and hash.

---

## 13. Open questions

| ID | Question | Owner |
|---|---|---|
| Q-008 | Minimum Next.js version for build-graph evidence? | PLAN-4: 16.1 partial, 16.4 full |
| Q-014 | Is the `next analyze export` schema documented per version? | PLAN-4 |
| Q-015 | How are Turbopack virtual modules represented? | Resolved: `[project]/` sources, `[externals]/` shims, `[next]/`/`[turbopack]/` framework modules, `[client-fs]/`/`[output]/` assets; proxy modules name their source. Non-project modules are excluded from coverage. |
| Q-016 | Does Shrike run `next build` itself? | PLAN-4: only with `--build` |
| Q-027 | Is the client-reference manifest available across versions? | PLAN-4 |
| Q-042 | How are monorepo builds with multiple Next.js apps handled? | PLAN-4 |
| Q-043 | How are route groups and parallel routes attributed? | PLAN-4 |
