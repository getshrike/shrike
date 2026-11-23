<h1 align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="public/shrike-logo-paper.svg">
    <source media="(prefers-color-scheme: light)" srcset="public/shrike-logo.svg">
    <img src="public/shrike-logo.svg" alt="Shrike" width="440">
  </picture>
</h1>

<p align="center">
  <strong>Verified dead-code cleanup for TypeScript and JavaScript.</strong><br>
  Delete what a pull request orphaned. Prove the project still passes its checks.
</p>

<p align="center">
  <a href="https://github.com/getshrike/shrike/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/getshrike/shrike/actions/workflows/ci.yml/badge.svg?branch=main"></a>
  <a href="https://www.npmjs.com/package/getshrike"><img alt="npm version" src="https://img.shields.io/npm/v/getshrike.svg"></a>
  <a href="https://www.npmjs.com/package/getshrike"><img alt="Node.js 22 or newer" src="https://img.shields.io/node/v/getshrike.svg"></a>
  <a href="LICENSE"><img alt="MIT license" src="https://img.shields.io/badge/license-MIT-blue.svg"></a>
</p>

<p align="center">
  <a href="#quick-start">Quick start</a> ·
  <a href="#how-it-works">How it works</a> ·
  <a href="#commands">Commands</a> ·
  <a href="#framework-coverage">Coverage</a> ·
  <a href="#documentation">Docs</a>
</p>

## Why Shrike

Agents and pull requests add code much faster than they remove it. A deleted
caller leaves its helper behind. A replaced data path leaves the old hook
behind. A removed route leaves its components behind. Tests and lint stay green
because the leftover code is unreachable, not wrong, and the next agent builds
on top of it.

Most dead-code tools stop at detection. Shrike owns the risky part: the removal
and the proof.

- **Whole-graph orphan delta.** Compares the merge-base graph with the PR head
  graph, so it finds code that became dead anywhere in the repository —
  including files the PR never touched.
- **Verified deletion batches.** Applies each batch in a branch worktree, runs
  the project's own checks against a baseline, bisects failures, and reverts
  only the culprit.
- **Portable receipt.** Records graph hashes, witnesses, patches, baseline and
  after checks, build evidence, and the outcome as content-addressed JSON.

## Quick start

Requires Node.js 22+. The npm package is `getshrike`; it installs the `shrike`
and `getshrike` commands and resolves a native engine for your platform.

```bash
npm install -D getshrike
npx shrike init
```

`init` writes `shrike.config.ts` and adds `.shrike/` to `.gitignore`. Start
read-only:

```bash
npx shrike scan                      # current-state dead-code evidence
npx shrike delta --base origin/main  # what this branch orphaned
npx shrike plan --base origin/main   # deletion batches and blast radius
```

`scan`, `delta`, and `plan` exit `1` when they find something, so they can gate
CI as-is. A real delta looks like this:

```console
$ npx shrike delta --base origin/main
Shrike delta: 1 newly dead symbol in 1 file
  src/format.ts#formatInvoice  -3 lines  [src/format.ts]
  src/format.ts  (file)
Cascade: 1 symbols, 1 files, 3 lines
```

When the plan looks right:

```bash
npx shrike prune --base origin/main
```

`prune` applies each batch in a worktree under `.shrike/worktrees/`, runs your
checks against a baseline, commits only batches that pass, and writes a receipt
per batch under `.shrike/receipts/`. It never commits to the base branch. Add
`--pr` to open a draft PR with `gh`.

## How it works

Shrike builds two deterministic snapshots — one at the merge base, one at the PR
head — and diffs their live sets:

```text
merge-base ─┐
            ├─ orphan delta → cascade → batches → verify → receipt
PR head ────┘
```

- **Orphan delta.** `live(base) − live(head)` gives code that was reachable
  before the PR and is not reachable now. Each item names the changed edge that
  removed its last witness (`removed-import`, `removed-call`, `removed-route`,
  ...).
- **Cascade and partition.** Tarjan SCC condensation closes over exclusive
  helpers; a conflict graph splits the result into independent batches with
  blast-radius caps.
- **Verification.** Each batch is applied in a worktree. Baseline checks run
  before the edit, the same checks run after, and the results are compared. A
  failure is bisected, the culprit reverted, and the remaining edits retried.
- **Receipt.** A batch that survives gets a content-addressed receipt that can
  be re-checked with `shrike verify`.

## What "verified" means

Verified means the project's configured checks passed after the removal,
relative to a baseline. It does not mean the code is semantically unused in
production, that the tests are adequate, or that external consumers cannot
import it.

Shrike fails closed. By default, an item is blocked rather than deleted if it is
an entry, public API, generated, unparsed, or reachable through an unknown edge;
blocked items are reported with a reason. The analyzer treats repository content
as data, never instructions, and never spawns a process or opens a socket. A
build graph can add liveness evidence but can never authorize a deletion.

## Commands

| Command | What it does |
| --- | --- |
| `shrike init` | Create `shrike.config.ts` and ignore `.shrike/` |
| `shrike scan` | Current-state evidence and deletion candidates |
| `shrike deps` | Unused and unlisted dependency evidence |
| `shrike delta --base <ref>` | Whole-graph PR orphan delta |
| `shrike plan --base <ref>` | Deletion batches, impact, and blast radius |
| `shrike prune --base <ref>` | Apply, verify, and commit batches in a branch worktree; `--pr` opens a draft PR |
| `shrike verify --plan <id>` | Re-run verification for a saved plan |
| `shrike proof <symbol>` | Witness path or proof trace for a symbol |
| `shrike receipt [id]` | Show a receipt |
| `shrike recover <snapshot>` | Restore files after a failed revert |
| `shrike evidence --build <path>` | Join a build-evidence export |
| `shrike hook install` | Install the post-agent hook (`--claude`, `--cursor`, or `--opencode`) |
| `shrike mcp serve` | Read-only MCP server over stdio |
| `shrike cache clean\|verify` | Cache maintenance |
| `shrike version` | Version and protocol information |

All read-only commands accept `--json`. Exit codes: `0` clean, `1` findings,
`2` usage/config, `3` verification failed (changes reverted), `4` could not run
(fail closed), `5` revert failed (manual recovery required).

## Framework coverage

Shrike protects framework-invoked code so it is not mistaken for dead code.
Framework entries are gated on the owning package declaring the dependency, so a
file that merely looks like a route in an unrelated package stays analyzable.

| Family | Covered conventions |
| --- | --- |
| Next.js App Router | `page`, `layout`, `route`, `loading`, `error`, `template`, metadata and icon files, middleware/proxy, instrumentation, route segment config, server actions, RSC boundaries |
| Next.js Pages Router | `pages/**` routes, `_app`/`_document`, `getStaticProps`/`getServerSideProps`/`getInitialProps`, route-tree conflicts |
| React / bundlers | `createRoot` and legacy roots, Vite `index.html` scripts, webpack entries, `import.meta.glob`, `require.context` |
| Routers | React Router, Remix, TanStack Router, Expo Router, Nuxt, SvelteKit |
| Tooling | Storybook, Ladle, React Cosmos, Cypress, Docusaurus, Gatsby, Metro platform variants |
| Packages | `package.json` `exports`/`main`/`bin`, workspace packages, public API surfaces |
| Config and tests | Statically named config and test entry modules; framework-generated types stay live |

## Integrations

- **Post-agent hook.** `npx shrike hook install --claude` (also `--cursor` and
  `--opencode`) installs a hook that runs after the agent stops. It computes the
  delta, runs fast checks against a baseline, and writes a bounded
  `CleanupBrief`. It never applies a deletion, has a 60-second budget, and
  degrades to delta-only on timeout.
- **GitHub Action.** [`packages/action/`](packages/action/action.yml) wraps the
  CLI. `mode: delta` is read-only and can fail a PR with `fail-on: newly-dead`
  or when `max-items` is exceeded. `mode: prune` produces a verified cleanup PR
  and refuses fork PRs.
- **MCP server.** `npx shrike mcp serve` exposes four read-only tools —
  `shrike_delta`, `shrike_plan`, `shrike_proof`, and `shrike_receipt` — over
  stdio. No tool mutates the repository, runs project checks, or reads `.env`.

## Performance

The engine is written in Rust and emits deterministic JSON: identical input
produces byte-identical output, regardless of thread count.

| Scenario | Budget |
| --- | ---: |
| 1,000 files, cold | ≤ 1.5 s |
| 1,000 files, warm | ≤ 400 ms |
| 10,000 files, cold | ≤ 6 s |
| 10,000 files, warm | ≤ 2 s |
| 10,000 files, peak RSS | ≤ 600 MB |

The harness and reported runs are in [`benchmarks/`](benchmarks/README.md).
Absolute times are machine-specific; every reported number includes the machine
and commit.

## Scope

Shrike is not a linter, a bundle optimizer, a type checker, a plugin catalogue,
or a cleanliness score. It does not auto-commit to `main`; every mutation happens
on a branch. It analyzes TypeScript and JavaScript with first-class Next.js and
React support, and will not grow into a multi-language analyzer.

## Documentation

| If you are… | Read |
| --- | --- |
| setting up the repo or looking for a command | [`docs/development.md`](docs/development.md) |
| learning what Shrike reports | [`docs/features.md`](docs/features.md) |
| learning the algorithms and invariants | [`docs/how-it-works.md`](docs/how-it-works.md) |
| deciding where a change belongs | [`docs/architecture.md`](docs/architecture.md) |
| adding or running tests | [`docs/testing.md`](docs/testing.md) |
| opening or reviewing a PR | [`docs/contributing.md`](docs/contributing.md) |
| preparing a release | [`docs/release.md`](docs/release.md) |

The full manual starts at [`docs/README.md`](docs/README.md).

## Project layout

```text
crates/        Rust engine: syntax, resolve, graph, delta, plan, evidence, protocol
packages/      TypeScript host: cli, verifier, receipts, agent, action, testkit
schemas/       JSON Schema contract between the two runtimes
fixtures/      Delta, verification, build-evidence, receipt, and corpus fixtures
benchmarks/    Deterministic generator and competitor harness
docs/          Development manual
```

## Contributing

Contributions are welcome. Read [`docs/contributing.md`](docs/contributing.md)
before opening a PR, and [`AGENTS.md`](AGENTS.md) if you are working with a
coding agent. In short: one concern per branch, Conventional Commit titles, and
the commands and results recorded in the PR.

To build from source, run `pnpm install`, then
`cargo build --release -p shrike-engine` and `pnpm run build:ts`. Setup, the
full gate list, and troubleshooting are in
[`docs/development.md`](docs/development.md).

## License

MIT. See [`LICENSE`](LICENSE).
