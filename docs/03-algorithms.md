# 03 — Algorithms

> **Parent:** [MASTER_PROMPT.md](../MASTER_PROMPT.md) · **Tracker:** [TRACKER.md](../TRACKER.md)
> **Status:** canonical · Authored 2026-11-20
> **Implementation status (2026-11-20):** implemented in `crates/shrike-graph`, `shrike-delta`, and `shrike-plan`; exercised by the 14 delta fixtures and `scripts/e2e-prune.mjs`. Impact currently reports files, lines, and symbols; route/byte impact is phase 4 work.

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
   Hidden directories are editor/tooling metadata except `.storybook`,
   `.rnstorybook`, and `.ladle`, whose configuration, preview, and provider
   modules are executable.
3. Read `pageExtensions` from each `next.config.*` and add the Markdown files
   that become routes: every `.md`/`.mdx` under `pages/` or `src/pages/`, and
   convention-stem files under `app/` or `src/app/`. A config only applies to
   its own directory, so a root config cannot absorb a nested app.
4. Normalize paths relative to the repository root.
5. Hash file contents with BLAKE3.
6. Detect workspace packages from `package.json` workspaces, `pnpm-workspace.yaml`,
   and `tsconfig` references.
7. Detect generated files from common markers and config; generated files are
   excluded from deletion candidates by default but still parsed for edges.
8. Sort files by normalized path. This order is the canonical merge order.

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

**Metro platform resolution.** When the owning package depends on `metro`,
`@react-native/metro-config`, or `react-native`, a neutral import probes each
platform variant before falling back to the base module, in Metro's order:
`ios`, `android`, `windows`, `web`, `native`, `macos`, `default`. A suffix is
inserted before the source extension (`Button.tsx` → `Button.ios.tsx`,
`Home` → `Home.native.tsx`). An extensionless import also probes each
platform's directory index (`Card/index.ios.tsx`) when no base index exists.
Every variant that exists is treated as a target
of the same specifier, so an imported named export stays live in each platform
file. Variants are not blanket entries: a platform file that no neutral import
reaches, such as `orphan.ios.tsx`, remains a deletion candidate, and unused
helpers in a reached variant remain analyzable. The gate is per package, so a
monorepo can contain both Metro and non-Metro packages.

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
| `import.meta.glob('./pages/*.tsx')` | dynamic → every matched module | `probable` |
| `require.context('./icons', true)` | dynamic → every module in the directory tree | `probable` |
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

`app/` and `src/app/` are resolved relative to the owning package, not to the
repository root or any nested path segment. A directory named `app` inside an
unrelated package is not a Next.js route. The owning package must declare
`next` in one of its dependency sections; a missing or unreadable
`package.json` fails open, while a manifest without `next` leaves the files
analyzable.

- `page`, `layout`, `template`, `loading`, `error`, `global-error`, `not-found`,
  `forbidden`, `unauthorized`, `route`, `default`; with `pageExtensions`
  containing `md` or `mdx`, the same convention stems are routes as Markdown,
  and their ESM imports keep rendered components live. A dotted value is one
  extension: with `["page.tsx"]`, `app/page.page.tsx` has the stem `page` and
  `app/layout.page.tsx` the stem `layout`, matching knip's AST-derived patterns
  and Next's own suffix strip. Only framework-invoked exports become live, so
  other exports in those files stay analyzable;
- `sitemap`, `robots`, `manifest`, `icon`, `apple-icon`, `opengraph-image`,
  `twitter-image`;
- image-route metadata: `size`, `contentType`, `alt`, `generateImageMetadata`
  (scoped to image-route stems, never global);
- `proxy.ts` (Next.js 16) and `middleware.ts` (15) exports;
- `instrumentation.ts` and `instrumentation-client.ts` at the package root or
  `src/`, with `register`, `onRequestError`, `onRequestInit`, and
  `onRouterTransitionStart` as the framework API. Other exports in those files
  stay analyzable, and a nested module that merely shares the name
  `instrumentation` is not a convention;
- `mdx-components.tsx` `useMDXComponents` and the MDX ESM boundary, only when
  the owning package or the workspace root declares `@next/mdx`: that package
  aliases `next-mdx-import-source-file` to the root or `src/` component map,
  while Next core does not load the file by name. An unreferenced map in a Next
  app without `@next/mdx` stays analyzable, and a missing manifest fails open;
- `"use server"` module exports under `serverActionPolicy: "protect"`. The
  default `graph` policy treats an action like any other export, so an action no
  module references is reported; an import, `action={fn}` binding, or any other
  reference keeps it live. Build evidence can also keep a referenced action live;
- `"use client"` module exports (client boundary);
- a `"use client"` module that exports a Next.js server-only, route-config, or
  route-handler name produces an advisory `invalid_client_export` warning, not
  a finding. The names are `metadata`, `generateMetadata`, `viewport`,
  `generateViewport`, `generateStaticParams`, `dynamic`, `dynamicParams`,
  `revalidate`, `fetchCache`, `runtime`, `preferredRegion`, `maxDuration`,
  `getServerSideProps`, `getStaticProps`, `getStaticPaths`, `GET`, `POST`,
  `PUT`, `PATCH`, `DELETE`, `HEAD`, and `OPTIONS`. The rule is gated on the
  project declaring `next` in the root or a workspace manifest, because a
  shared client module is compiled in the consuming Next app's context. A
  direct export is anchored on its declaration; a local `export { name }` is
  anchored on its specifier and reported once. Default, type-only, and
  re-export forms are excluded. The list matches fallow 3.45.0's
  `ILLEGAL_CLIENT_EXPORTS`; knip 5.88.1 does not report the condition;
- a `"use client"` or `"use server"` string after the leading prologue
  produces a `misplaced_directive` warning, not a finding. RSC bundlers only
  honor a directive before any other statement and silently ignore the rest,
  so the intended boundary never exists. The rule is gated on the project
  declaring `next`, `waku`, `@lazarv/react-server`,
  `react-server-dom-webpack`, `react-server-dom-vite`,
  `react-server-dom-parcel`, or `@vitejs/plugin-rsc` in the root or a
  workspace manifest, matching fallow 3.45.0's `RSC_BUNDLER_DEPS`. One warning
  per occurrence, anchored on the statement; `"use strict"` and nested strings
  are ignored;
- a barrel that directly re-exports both a `"use client"` origin and a
  server-only origin produces a `mixed_client_server_barrel` warning, not a
  finding. A server-only origin carries `"use server"`, imports `server-only`,
  `next/server`, `fs`/`fs/promises`/`child_process` (bare or `node:`), or a
  named `cookies`/`headers`/`draftMode` API (or namespace) from
  `next/headers`. Type-only re-exports and type-only imports are erased and do
  not count. The rule uses the same seven-bundler gate as
  `misplaced_directive`, names the first client and server origins in source
  order, and anchors on the earlier re-export. It follows fallow 3.45.0's
  `mixed_client_server_barrel` detector, with the type-only import exclusion
  as a deliberate precision improvement;
- a `"use client"` module whose runtime import cone reaches server-only code
  produces a `client_server_leak` warning, not a finding. The cone follows
  runtime static imports, re-exports, export-alls, and non-worker dynamic
  imports; type-only edges are erased. The sink predicate is the same one used
  by `mixed_client_server_barrel`: `server-only`, `next/server`,
  `fs`/`fs/promises`/`child_process` (bare or `node:`), or a named
  `cookies`/`headers`/`draftMode` API (or namespace) from `next/headers`. An
  all-action `"use server"` module stops the cone because its exports become
  action references at the boundary; a module with any non-action value export
  keeps walking. A `next/dynamic(..., { ssr: false })` edge is skipped when the
  same specifier has no runtime static edge, because the target is
  intentionally client-only. The rule uses the same seven-bundler gate, emits
  one warning per client root, anchors on the first import of the shortest
  path, and names the sink plus the full path. It follows fallow 3.45.0's
  `client_server_leak` detector;
- metadata exports: `metadata`, `generateMetadata`, `viewport`,
  `generateViewport`, `generateStaticParams`, `generateSitemaps`;
- dynamic param matching (Next.js 16.4): `unstable_paramMatching` and
  `unstable_generateParamMatching` on `page` and `layout` modules. Route
  handlers and other App Router stems do not consume them, so the same names
  stay analyzable there;
- segment config: `dynamic`, `dynamicParams`, `revalidate`, `fetchCache`,
  `runtime`, `preferredRegion`, `maxDuration`; Cache Components config
  `ensureStatic` (16.4), `instant`/`prefetch` (16.3) and their
  `unstable_instant`/`unstable_prefetch` predecessors (16.0-16.2);
- a root `server.{js,mjs,cjs,ts,mts,cts}` in a package that declares `next` is
  the custom HTTP server that replaces `next start`. Its module is an entry, so
  top-level work and imports stay live, but its named exports are not read by
  the framework and remain analyzable;
- modules referenced only by a relative path in `next.config.*`
  (`images.loaderFile`, `cacheHandler`/`cacheHandlers`, Turbopack loaders and
  aliases): the module and all its exports are entries;
- App Router files that Next.js cannot route unambiguously produce error
  diagnostics, not findings. `route_collision` fires when two `page`/`route`
  leaves in one app root resolve to the same URL, including a page and a route
  handler. `dynamic_segment_name_conflict` fires when sibling dynamic segments
  at one tree position use different spellings (`[id]` vs `[slug]`). Route
  groups are transparent, parallel `@slot` segments fork the namespace, and
  private `_folder` and intercepting `(.)`/`(..)`/`(...)` subtrees are excluded.
  Both checks require the owning package to declare `next`; unlike fallow's
  project-wide gate, an app-shaped directory in a React-only workspace package
  is not a Next app root. The route-tree primitive follows fallow 3.45.0's
  `route_tree.rs`, and the URL/bucket rules match its `route_collision` and
  `dynamic_segment_name_conflict` detectors.

### 9.2 Next.js Pages Router

`pages/` and `src/pages/` are likewise package-relative.

- `pages/**` default exports;
- `pages/**` Markdown files when `pageExtensions` includes `md` or `mdx`; their
  ESM imports are reachability edges;
- `pages/api/**` HTTP method exports;
- `_app`, `_document`, `_error`;
- `getServerSideProps`, `getStaticProps`, `getStaticPaths`, `getInitialProps`.

### 9.3 React

- application roots (`createRoot`, `hydrateRoot`);
- React 17 legacy roots: `ReactDOM.render` and `ReactDOM.hydrate`, matched only
  when the object is a default or namespace import from `react-dom`, or when a
  named `render`/`hydrate` import from `react-dom` is called. A
  testing-library `render(<App />)` is not a root;
- route configuration objects;
- lazy imports;
- context providers that are mounted at a root.

### 9.4 Vite and webpack

Vite projects are detected from a `vite`, `vitest`, or `rolldown-vite`
dependency in the owning package. The framework then adds these entries:

- `index.html` itself and each `<script type="module" src="...">` it names.
  External URLs, inline-only scripts, commented-out tags, classic scripts, and
  source templates that still contain `%...%` placeholders are ignored. The
  whole export surface of a script module is live because the page can read any
  property of its namespace;
- `src/main.{ts,tsx,js,jsx}` and `src/index.{ts,tsx,js,jsx}` at the package
  root, with their whole export surface. This is the vanilla/Vue bootstrap that
  never calls React's `createRoot` and need not appear in `index.html`.

HTML files are part of the default include set (`**/index.html`) so their
content participates in the snapshot cache key. Their script sources are
ordinary side-effect imports; the entry rules above decide which modules are
reachable.

Webpack configurations are read statically from `webpack.config.*` and
target-specific names such as `webpack.prod.config.*` or
`config/webpack.client.*`. The extractor understands:

- `entry` as a string, an array, an object of strings/arrays, or webpack 5
  `{ import: ... }` descriptors;
- `context` as a string or a `path.resolve`/`path.join` expression, including
  simple `const` aliases;
- entries written as `path.resolve(__dirname, ...)`.

Each named module and its whole export surface becomes an entry. Paths are
anchored at `context`, or at the package root when no context is declared.
Dynamic expressions, absolute filesystem paths, and environment values are
skipped rather than executed; Knip can evaluate those because it runs the
configuration in Node.

### 9.5 Packages

- `package.json` `exports`, `main`, `module`, `types`;
- `index.ts` public surface when the package is a library;
- declaration files are public by default.

### 9.6 Tests and config

- test files, setup files, fixtures. A test module is an entry because the runner
  discovers the file, but its exports are ordinary helpers and follow the graph;
  only exports imported by another module stay live. `__mocks__` modules keep
  their full export surface because Jest loads them by convention, and type-test
  files keep theirs because they exist to assert the exports;
- standalone examples, demos, playgrounds, and benchmarks. Modules under a root
  `examples/`, `example/`, `demo/`, `playground/`, `playgrounds/`, or
  `benchmarks/` directory are entries because they are executed directly. Their
  exports are helpers and follow the graph, so an unimported export is
  reportable while top-level work and imports stay live;
- package scripts. A file named by `package.json` `scripts` or `bin` is an
  entry, and its exports follow the graph. A file under `scripts/` is not an
  entry by location, a documented `Usage:`/`Run:` header, or a shebang alone:
  those are documentation, not references. An unreferenced one-off tool is
  therefore an unused file with reportable declarations, matching Knip 5.88.1
  and fallow 3.45.0;
- Storybook `.{storybook,rnstorybook}/**` config and `*.stories.*` /
  `*.story.*` files. Story modules expose every export to Storybook. When the
  owning package depends on `storybook`,
  `@storybook/*`, or `@nrwl/storybook`, every `.mdx` page is also an entry:
  Storybook's default story glob includes all MDX, and docs pages render the
  components they import. The `stories` array in `.storybook/main.*` is read
  statically, including `{ directory, files }` specifiers, custom suffixes such
  as `*.demo.tsx`, and extglob alternation such as `*.@(js|jsx)`. Patterns are
  anchored at the main-config directory. Computed and negated patterns are
  skipped; preview, manager, and decorator modules are entries and their
  imports stay live;
- Ladle stories and provider modules. When the owning package depends on
  `@ladle/react`, `.ladle/components.{js,jsx,ts,tsx}` and `.ladle/config.*` are
  entries, and every export of the components file is part of the provider
  surface. Story modules come from Ladle's default
  `src/**/*.stories.{js,jsx,ts,tsx,mdx}` glob, or from literal string/array
  `stories` values in `.ladle/config.*` anchored at the package root. A literal
  `viteConfig` path is also an entry, since Ladle executes it. Computed story
  values fall back to the default glob; computed Vite paths are skipped. The
  hidden `.ladle` directory is walked because it holds executable project
  files;
- React Cosmos fixture and decorator files. When the owning package depends on
  `react-cosmos`, Cosmos's own user-module patterns are entries:
  `**/<fixturesDir>/**/*.{js,jsx,ts,tsx,md,mdx}`,
  `**/*.<fixtureFileSuffix>.{js,jsx,ts,tsx,md,mdx}`,
  `**/<fixtureFileSuffix>.{js,jsx,ts,tsx,md,mdx}`, and
  `**/cosmos.decorator.{js,jsx,ts,tsx}`. `fixturesDir` (default `__fixtures__`)
  and `fixtureFileSuffix` (default `fixture`) are read from
  `cosmos.config.json`; `ignore` patterns are honoured. Fixture modules expose
  every export to Cosmos, while only the decorator default export is invoked;
  named helpers in a decorator file stay analyzable. The config is hashed into
  the snapshot cache key even though it is not parsed as a module;
- Docusaurus documentation pages. When the owning package depends on
  `@docusaurus/core`, `@docusaurus/preset-classic`, or `docusaurus`, every
  Markdown/MDX file under `docs/` or `blog/` and every `versioned_docs/` page
  (Markdown, MDX, or React) is an entry. Docusaurus renders these modules by
  directory convention, so their imports stay live while unused exports in the
  imported components remain analyzable. `.md` files are matched only when the
  configured analysis include covers them; the default include starts at
  `.mdx`;
- Gatsby page templates and pages. When the owning package depends on `gatsby`
  or `gatsby-cli`, `gatsby-node.*` is parsed statically for
  `createPage({ component })` registrations. Literal paths, static template
  literals, `path.resolve`/`path.join`, `require.resolve`, local variable
  bindings, and relative component imports are resolved. A registered module
  is an entry, and only Gatsby's page API exports (`default`, `query`, `Head`,
  `getServerData`) are framework-invoked; other exports in the template stay
  analyzable. When a component value cannot be proved, every module under
  `src/templates/` in that package is an entry instead. Every module under the
  package's `src/pages/` directory is also a page entry, with the same API
  surface. A nested `src/gatsby-node.ts` is not a Gatsby node file;
- Cypress specs and support files. When the owning package depends on
  `cypress`, `cypress.config.*` is read statically. E2E specs come from
  `e2e.specPattern` (default `cypress/e2e/**/*.cy.{js,jsx,ts,tsx}`) minus
  `e2e.excludeSpecPattern`, and the support module comes from
  `e2e.supportFile` (default `cypress/support/e2e.{js,jsx,ts,tsx}`);
  `supportFile: false` disables it. Component patterns apply only when the
  config declares a `component` object, because the component default
  `**/*.cy.*` would otherwise protect unrelated files in an e2e-only project.
  Patterns are anchored at the config's directory. Only literal strings are
  trusted; a computed value falls back to Cypress's documented default. Specs
  and support are module entries: Cypress executes them, while their unimported
  named exports remain analyzable;
- `next.config`, `postcss.config`, `tailwind.config`, `eslint.config`,
  `tsconfig`;
- test runner config.

### 9.7 MDX

MDX is analyzed at its ESM boundary only: top-level `import` and `export`
statements are parsed with byte offsets preserved, while the Markdown/JSX body
is opaque. Every MDX declaration is treated as referenced by the body, so an MDX
component is never proposed for deletion. Imports from MDX are normal graph
edges, which keeps the components it renders live. Fenced code blocks and YAML
frontmatter are ignored.

### 9.8 Additional framework routers

The same entry rule applies to conventions with their own compilers:

- Remix / React Router: `app/root`, `entry.client`, `entry.server`, and
  `app/routes/**` loader/action/meta/error exports. React Router framework mode
  is detected package-wide from an `app/routes.ts` (or `react-router.config.*`)
  that imports `react-router`; every module under `app/routes/**` is then a
  route even when it imports only its generated `./+types/*` declarations.
  When `react-router.config.*` sets `appDirectory`, the root, entry, and
  `routes/` tree move with it. Route modules named by the route config map
  (`index(...)`, `route(...)`, `layout(...)`, or a `file:` property) stay
  protected even when they live outside `<appDirectory>/routes/`, matching the
  path the router loads. Remix is detected package-wide from
  an `app/root` that imports `@remix-run/*`, so route modules need no imports of
  their own. The root route alone may also export `Layout`, the document shell,
  and `middleware` / `clientMiddleware` (including their `unstable_` spellings
  in React Router 7.3-7.8) are framework-invoked like `loader` and `action`.
  Remix v1 route hooks `CatchBoundary`, `shouldReload`, and
  `unstable_shouldReload` remain protected for projects on that API. A root
  `server.{js,jsx,ts,tsx,mjs,cjs}` in the detected package is also an entry:
  Remix apps with a custom server execute it instead of `remix-serve`, so its
  top-level work and imports stay live. Its named exports are not read by the
  framework and remain analyzable;
- TanStack Router: `Route`/`route` objects in modules that import a route
  factory (`createFileRoute`, `createLazyFileRoute`, `createRootRoute`, or
  `createServerFileRoute`), so a configured `routesDirectory` is covered; the
  `src/routes/**` convention remains as a fallback;
- Expo Router: every module under `app/` or `src/app/` once a layout exists, or
  under the `root` configured by the `expo-router` plugin in `app.json` /
  `app.config.*`; `+api` routes expose HTTP method handlers, `+native-intent`
  exposes `redirectSystemPath`/`legacy_subscribe`, and `+middleware` exposes
  `default` and `unstable_settings`. Local config plugins named by relative
  path in the app config `plugins` array (including a local
  `const plugins = [...]`, `require.resolve`, and tuple options) are entries,
  and the whole plugin module surface is protected because Expo loads it
  without an import;
- SvelteKit: `src/routes/**/+page|+layout|+server|+error` exports and route
  modules (`.svelte` files have a module entry even though their body is not
  parsed);
- Astro: `src/pages/**` endpoint handlers and page modules;
- Nuxt: `server/api`, `server/routes`, `server/middleware`, `plugins`,
  `middleware`, `app.vue`, and `pages/**` route modules.

### 9.9 Internal-only exports

An exported symbol that is live only because its own file references it is
reported in `internalOnly`, separately from `candidates`. It is not a deletion
candidate: the export may be part of a public surface the graph cannot see, and
same-file re-export aliases propagate the proof. A type imported externally
also makes the types in its shape externally visible, so same-file type-to-type
references propagate the proof as well. Value references do not: a helper used
by an externally called function stays internal and can be de-exported. This
gives parity with unused-export tools without turning a conservative graph into
a false positive.

External visibility is reachability-aware. A cross-file import or re-export
only counts when its source symbol is live. An export referenced only by a dead
module, or by a dead alias inside a live barrel, is still internal-only: code
that cannot run must not keep a same-file-only export out of the report.
Unknown source keys fail closed and count as external.

### 9.10 Dependency evidence

Dependency analysis is deliberately asymmetric: it prefers a false negative to a
false positive. A declared package is kept when any of these hold:

- a runtime specifier resolves to it;
- a type-position reference names it (`import("pkg").Config`, JSDoc
  `@type {import("pkg").X}`); type positions never create `unlisted` findings,
  because a tool config's types are often supplied transitively;
- it is named by a package script, a recognized tool config (ESLint, PostCSS,
  Tailwind, Next.js `turbopack.rules` loaders, ...), or a CSS tool directive;
- it is an `@types/*` package whose runtime target is used, an ambient global
  package, or listed in `compilerOptions.types`;
- it is a requirement of a used package (peer/optional/known ecosystem map);
- a framework entry implies it (`next`/`react`/`react-dom` for Next.js,
  `astro` for Astro, and so on).

An import that is only available through a declared dependency's peer or
optional requirements is reported as `transitive` (informational), not
`unlisted`. `shrike prune` never removes dependencies.

Dependency usage is reachability-aware. An import or type reference from a
module whose `__module__` symbol is not live does not keep a dependency used;
when every importer is dead, the dependency is reported with those importers as
evidence. If liveness data or the module symbol is unavailable (for example a
parse failure), the module is treated as live, so this rule fails closed.

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
for each symbol widened by an unknown scope: mark live, queue.push_back(symbol)
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
direction. Widening changes the seed set, never the reachability rule: a symbol
made live by an unknown scope is expanded like an entry. Otherwise a live
export's helpers are reported dead even though the export calls them.

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

**Invariant F6:** every live symbol is expanded exactly once. Symbols widened by
an unknown scope are queued, so their outgoing references are traversed.

**Rule F7:** every dead named re-export alias is a candidate, whether its chain
ends at a live symbol or at a dead declaration. Reporting both the declaration
and each alias is what makes the deletion batch complete: removing only the
declaration would leave the barrel specifier naming a symbol that no longer
exists. The patch removes the alias specifier, not the whole export statement,
so live siblings in `export { live, dead }` survive.

The rule also covers local `export { X }` and `export { X as Y }` lists. If `X`
is a local declaration the alias points at it; if `X` is an imported binding
the alias points at the resolved import target. This keeps `import { X } from
"./impl"; export { X };` on the same chain as `export { X } from "./impl";`,
so a dead re-export of a live import is reported instead of disappearing.
`export * from "./impl"` has no per-name alias symbol; it stays a module edge
and is not reported as a specifier.

### 10.4 Unused files

**Rule F8:** a file is an unused-file finding when no entry names it, no other
module has an incoming edge to any of its symbols, its module symbol is not
live, it parsed successfully, and it is neither public API nor generated. This
is deliberately stricter than "every declaration is a candidate": a barrel
imported by a live module is used even when every export it forwards is dead,
and a file imported only by dead code waits for the deletion cascade instead of
being reported at the root. An unimported side-effect-only module has no
non-module candidates, so this list is also the only way to surface it. Apply
deletes the whole file when all of its declarations are removed and its module
is not live, which is the batch a file finding describes.

### 10.5 Proof trace

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
| `import.meta.glob(pattern)` | glob | `probable` | edges to every analyzed file matching the pattern; all of a matched module's exports are live |
| `import.meta.glob([positive, '!exclude'])` | glob with excludes | `probable` | as above, minus files matching an exclude |
| `require.context(dir, recursive)` | directory glob | `probable` | `dir/**/*` when recursive, `dir/*` when not |
| `import(someVar)` in package P | package scope | `unknown` | all files in P are live |
| `import(someVar)` at repo root | repo scope | `unknown` | all files are live; deletion is refused |
| `obj[key]` with literal keys | key set | `probable` | edges to the values |
| `obj[expr]` | module scope | `unknown` | all exports of the module's namespace are live |
| `globalThis[name]` | repo scope | `unknown` | all files are live; deletion is refused |

Bundler globs follow the importer: `./` and `../` patterns move with the
importing file, `/` starts at the repository root, and tsconfig aliases (plus
the common `@/` and `~/` source aliases) resolve through the same path map as
static imports. Negative Vite patterns (`!...`) exclude matches from every
positive pattern in the same call; a leading `**` exclude is anchored at the
common base of the positive patterns, matching Vite. A non-literal pattern or a
pattern this resolver cannot place widens to repo scope instead of guessing.
All exports of a matched module stay live because reading the glob result is an
arbitrary namespace use; only modules that no positive pattern matches remain
candidates.

Worker and worklet constructors load a module by URL instead of importing it.
The bundler still includes the target and its top-level code runs, so these are
module edges too:

| Form | Scope | Certainty | Effect |
|---|---|---|---|
| `new Worker(new URL('./w.ts', import.meta.url))` | literal URL | `certain` | edge to `w.ts` with kind `worker` |
| `new Worker('./w.js')`, `new SharedWorker(...)` | literal | `certain` | edge to the module |
| `navigator.serviceWorker.register('./sw.js')` | literal | `certain` | edge to the service worker module |
| `audioWorklet.addModule(...)`, `paintWorklet`, `layoutWorklet`, `animationWorklet`, `worklet` | literal | `certain` | edge to the worklet module |
| `importScripts('./a.js', './b.js')` | literal list | `certain` | one edge per script |
| `navigator.serviceWorker.register('/sw.js')`, `new Worker('https://cdn/w.js')` | absolute or scheme URL | none | runtime asset; no edge and no widening |
| a computed URL, a non-`import.meta.url` base, or an unparsed argument | package/repo scope | `unknown` | fail closed; all files in scope stay live |

A worker-loaded module is an entry: its `__module__`, every export, and
everything its top-level code references stay live. The `worker` edge kind
keeps proof traces and delta attribution explicit; delta maps it to
`changed-dynamic` because the triggering change is still a dynamic module
reference. An absolute or scheme URL (`/sw.js`, `https:`, `data:`, `blob:`) is
served at runtime and cannot expose an analyzed module, so it is ignored for
reachability and dependency evidence; only relative worker URLs are resolved
against the project. A URL with a base other than `import.meta.url` is a runtime location,
not a module this analysis can place, so it widens instead of guessing.

A literal dynamic import also narrows which exports stay live when the consumed
names are statically visible:

| Form | Live exports |
|---|---|
| `const { a, b } = await import('./m')` | `a` and `b` only |
| `const c = (await import('./m')).c` | `c` only |
| `import('./m').then((m) => m.a)` | `a` only |
| `import('./m').then((m) => { m.a(); m.b(); })` | `a` and `b` only |
| `import('./m').then(({ a, b }) => ...)` | `a` and `b` only |
| `const [{ a }, { b }] = await Promise.all([import('./a'), import('./b')])` | `a` and `b` only; a bare identifier element binds the namespace and widens its module |
| `const ns = await import('./m'); ns.a(); ns.b;` | `a` and `b` only |
| `const ns = await import('./m')` with a bare, computed, exported, or missing use of `ns`; a `let`/`var` binding; a `.then` callback that uses the namespace as a value; rest/spread destructuring; or an empty pattern | every export |
| a name that does not resolve through the target's re-exports | every export |

A `.then` callback narrows only when it is the first argument and the import is
the direct receiver. Every use of its namespace parameter must be a static
member read; a bare reference, computed access, spread, call, or return widens
the import. Chained callbacks such as
`import('./m').then((m) => m.a).then((a) => a.b)` receive the first callback's
result, not the module namespace, so only the first callback contributes names.

A `const ns = await import('./m')` binding narrows to the names read through
static members anywhere in the module. Only `const` qualifies: `let` and `var`
can be reassigned, so later member reads do not prove which value is read. A
bare reference, computed access, export of the binding, or no member read at all
widens. The post-visit scan is name-based; a shadowing local can add names but
cannot hide a real bare or computed use, so collisions stay conservative.

**Rule G5:** export narrowing fails closed. Any shape that could use the
namespace as a value, including `let`/`var` namespace bindings, a `.then`
callback that reads the namespace through anything other than static member
accesses, a `Promise.all` rest element, and any unresolved name widens to every
export instead of guessing.

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
