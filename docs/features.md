# Features

Shrike is the cleanup layer for AI-built TypeScript apps. It measures the dead
weight left behind by agent changes and PRs, then removes what the engine proves
is orphaned and runs your checks before and after. It is not a linter:
detection tools stop at "this looks unused"; Shrike takes responsibility for the
risky part — deleting the code and proving the project still builds. This page
is the catalogue of what that means in practice. See
[how-it-works.md](how-it-works.md) for the algorithms.

The headline metric is **dead weight %**: removable source lines divided by
analyzed non-generated source lines. Dependencies are reported separately and
are advisory; Shrike never removes a dependency automatically.

## The five pillars

| Pillar | What you get | Where it lives |
| --- | --- | --- |
| **Whole-graph PR orphan delta** | Code that became dead anywhere in the repo because of a change — including files the PR never touched | `shrike-delta`, `shrike-graph` |
| **Verified deletion batches** | Independent batches applied in a worktree, checked against a baseline, bisected and reverted on failure | `packages/verifier`, `packages/cli` |
| **Portable receipt** | Content-addressed JSON proof: graph hashes, witnesses, patches, checks, outcome | `schemas/receipt.schema.json`, `packages/receipts` |
| **Build-graph evidence** | Optional Next.js build evidence that suppresses false positives and reports route/byte impact | `shrike-evidence`, `packages/cli` |
| **Rust speed and determinism** | 10,000-file graphs in seconds; identical input produces byte-identical JSON | `crates/`, deterministic snapshots |

The wedge is not "we find dead code". It is: *Shrike finds what this PR
orphaned, deletes it in verified batches, and gives you a receipt that proves
the app still builds.*

## What it reports

### Deletion candidates

- Unreferenced exported symbols (functions, classes, variables, types, enums,
  namespaces).
- Dead named re-export aliases, **including aliases whose target declaration is
  also dead**. The patch removes the alias specifier, so live siblings in
  `export { live, dead }` survive.
- Whole unused files: a parsed, non-public, non-generated file with no entry, no
  incoming edge to any symbol, and a dead module symbol.
- Cascaded closures: a component and its exclusive helper are removed together,
  not left as dangling imports.

A symbol that is an entry, an unparsed file, public API, generated, or reachable
through an `unknown` edge is **blocked**, never deleted. Blocked items are
reported with a reason.

### Advisory diagnostics

Diagnostics are warnings, never deletion candidates. Categories used by the CLI:

| Category | Examples |
| --- | --- |
| `boundary` | `invalid_client_exports`, `invalid_server_exports`, `misplaced_directives`, `mixed_client_server_barrels`, `client_server_leaks` |
| `routing` | `route_conflicts`, `pages_router_conflicts`, `pages_router_exports`, `segment_conflicts` |
| `react` / `next` | `unused_component_props`, `absent_component_props`, `unused_page_data_keys` |
| `types` | `unused_enum_members`, `unused_class_members`, `unused_namespace_members` |
| `build-evidence` | evidence completeness and status problems |
| `parse` / `unresolved-import` / `asset-import` | parse failures and unresolved or non-code imports |

### Delta triggers

For each newly dead symbol, the delta names the changed edge that removed its
last witness: `removed-import`, `removed-call`, `removed-jsx`, `removed-route`,
`removed-re-export`, `changed-dynamic`, or `indirect` (closest changed ancestor).
A rename produces one of the import/re-export triggers, not a trigger of its own.
The tool reports a fact about the graph, not a guess about intent.

## Command surface

All read-only commands support `--json`. Exit codes: `0` clean, `1` findings,
`2` usage/config, `3` verification failed (changes reverted), `4` could not run
(fail closed), `5` revert failed (manual recovery required). `delta --fail-on
never` exits `0` even when it reports findings.

| Command | Purpose | Mutates |
| --- | --- | --- |
| `shrike report [--base <ref>]` | Dead-weight report, plain or interactive | no |
| `shrike scan` | Current-state evidence report and candidates | no |
| `shrike deps` | Unused / unlisted dependency evidence | no |
| `shrike why <path#Name>` | Human proof for one symbol | no |
| `shrike proof <path#Name>` | Witness path or proof trace for a symbol | no |
| `shrike delta --base <ref>` | Whole-graph PR orphan delta; `--fail-on never` exits `0` | no |
| `shrike plan --base <ref>` | Batches, impact, blast radius | no |
| `shrike clean [--base <ref>]` | Alias of `prune`; verified branch cleanup | yes (worktree) |
| `shrike clean --all` | Explicit current-state cleanup | yes (worktree) |
| `shrike prune --base <ref>` | Apply, verify, commit; `--pr` opens a draft PR | yes (worktree) |
| `shrike verify --plan <id>` | Re-run verification for a saved plan | worktree |
| `shrike receipt <id>` | Show a receipt | no |
| `shrike recover <snapshot>` | Restore files after a failed revert | yes |
| `shrike evidence --build <path>` | Join a build-evidence export | no |
| `shrike init` | Scaffold a config | config |
| `shrike install` | Write the agent skill directory and optional hook | config |
| `shrike ci install` | Write the GitHub Actions workflow | config |
| `shrike hook install --claude|--cursor|--opencode`, `shrike hook run`, `shrike hook uninstall` | Install / run / remove a bounded post-agent hook | config / cache |
| `shrike mcp serve` | Read-only MCP server over stdio | no |
| `shrike cache clean|verify` | Cache maintenance | cache |
| `shrike version` | Version and protocol information | no |

Commands are loaded lazily from `packages/cli/src/main.ts`; adding one means
adding a case there, not editing a central registry.

## Integration surfaces

### Agent skill and adapters

`shrike install` writes the canonical `SKILL.md` and its four references under
`.agents/skills/shrike/`, so an agent can load the short body first and read a
reference only when it needs exact flags, workflows, safety boundaries, or
setup detail. Target adapters point at the portable skill instead of duplicating
it. The installer is idempotent, previewable with `--dry-run`, and requires
`--force` to replace local edits; a conflict blocks the whole skill directory so
a new `SKILL.md` never ships beside stale references. `--no-skill` suppresses
every skill file. The same directory can be installed with
`npx skills add getshrike/shrike`.

### Post-agent hooks

After an agent finishes a task, the hook computes the delta, runs the fast
checks (typecheck and lint) against a baseline, and writes a bounded
`CleanupBrief`. It **never applies a deletion**. Budget: 40 s fast checks and
60 s total (the delta child process is capped at 55 s); on timeout it degrades
to delta-only and exits 0. Events: Claude Code `Stop`, Cursor `afterFileEdit`
or `stop`, OpenCode `session.idle`. The brief is capped at 20 items and 32 KB
and treats all
repository-derived strings as untrusted data.

### GitHub Action

`mode: delta` is read-only and fails on `fail-on: newly-dead` or above
`max-items`. `mode: prune` produces a verified cleanup PR and refuses fork PRs.
Permissions are `contents: read` for delta and `contents: write` +
`pull-requests: write` for prune; no `actions: write`. The Action is a thin
wrapper over the CLI.

### MCP server

Opt-in (`shrike mcp serve`). Four read-only tools: `shrike_delta`,
`shrike_plan`, `shrike_proof`, `shrike_receipt`. Only `shrike_plan` writes plan
artifacts under `.shrike/`; no tool edits project source, runs checks, reads
`.env`, or returns more than 256 KB.

### Distribution

One CLI package (`getshrike`, with `shrike` and `getshrike` bins) resolves a
native engine from `SHRIKE_ENGINE`, a repository build, the installed platform
package, then `PATH`. Packaged binaries require a SHA-256 sidecar; releases
add attestations, keyless signing, and npm provenance. See
[release.md](release.md).

## Framework and entry coverage

Shrike protects framework-invoked code so it is not mistaken for dead code.
These are the major entry families; the full per-version matrix lives in the
fixtures and in [architecture.md](architecture.md#where-to-make-a-change).

| Family | Covered conventions |
| --- | --- |
| Next.js App Router | `page`, `layout`, `route`, `loading`, `error`, `template`, metadata and icon files, middleware/proxy, instrumentation, route segment config, server actions, RSC boundaries |
| Next.js Pages Router | `pages/**` routes, `_app`/`_document`, `getStaticProps`/`getServerSideProps`/`getInitialProps`, route-tree conflicts |
| React / bundlers | `createRoot`/legacy roots, Vite `index.html` scripts, webpack entries, `import.meta.glob`, `require.context` |
| Routers | React Router, Remix, TanStack Router, Expo Router, Nuxt, SvelteKit |
| Tooling | Storybook, Ladle, React Cosmos, Cypress, Docusaurus, Gatsby, Metro platform variants |
| Packages | `package.json` `exports`/`main`/`bin`, workspace packages, public API surfaces |
| Config and tests | statically named config/test entry modules; framework-generated types stay live |

Framework entries are gated on the owning package declaring the dependency, so
a file that merely looks like a route in an unrelated package stays analyzable.

## Trust model and limits

| Layer | Trusts | Does not trust |
| --- | --- | --- |
| Analyzer | File contents as data | Repository content as instructions; never spawns a process or opens a socket |
| Config | The project's own `shrike.config.ts` (evaluated in a child process with a timeout and bounded environment) | Network-fetched config, ambient secrets |
| Verifier | The project's configured checks | Unconfigured scripts discovered by name |
| Receipt | Content hashes | A human assertion that checks passed |

"Verified" means the project's configured checks passed after removal relative
to a baseline. It does **not** mean the code is semantically unused in
production, that the tests are good, that the build is byte-identical, or that
external consumers cannot use it. The receipt states exactly what was checked.

Non-goals: bundle optimization, a plugin catalogue, multi-language analysis, a
cleanliness score, a replacement type checker, a generic dead-code MCP scanner,
auto-committing to `main`, and a hosted dashboard. A build graph can add
liveness but can never authorize a deletion.
