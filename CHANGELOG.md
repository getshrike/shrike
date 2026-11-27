# Changelog

All notable changes to Shrike are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.1.0] — 2026-11-27

The agent-first cleanup release: dead-weight reporting, verified `clean` and
current-state cleanup, an installable agent skill, CI installation, and a public
site with consumer docs.

### Added

- `shrike report`: a deterministic dead-weight report for the current tree or
  a branch delta, with a schema-valid `--json` form and a plain renderer for
  CI, pipes, and agent sessions.
- An interactive, read-only report view loaded lazily from `dist/tui.js`; it
  never loads for JSON, CI, non-TTY, agent sessions, or terminals smaller than
  40 columns by 10 rows, and the plain report is the fallback.
- `shrike clean` as the public name for the verified branch cleanup workflow,
  with `prune` kept as an alias, and `shrike why` as the human proof view over
  the existing `proof` data.
- `shrike clean --all`: explicit current-state cleanup through the same
  worktree, baseline, checks, bisection, revert, and receipt pipeline, with
  whole-file removal and a dry-run plan.
- `shrike install`: a portable agent skill directory with `SKILL.md` and four
  references, target adapters, managed blocks, optional hooks, and opt-in
  package scripts, with dry-run, idempotency, path safety, all-or-nothing
  conflict handling, and `--json` output.
- `shrike ci install`: an advisory-by-default GitHub Actions workflow with
  `contents: read`; `--gate` makes newly dead code blocking and adds
  `pull-requests: write`.
- `schemas/cleanup-report.schema.json`, report fixtures, and the report, TUI,
  current-state, install, and CI-install test suites.
- Docs for the report, report format, current-state cleanup, branch cleanup,
  exit codes, agent installation, and CI installation.

### Changed

- The README, root docs, CLI help, package description, and Mintlify pages now
  lead with the agent-first cleanup story and the dead-weight metric.

### Fixed

- Deletion planning keeps a file's selected declarations and their same-file
  users together, attaches every dead re-export specifier to its declaration,
  and no longer counts the same file twice against the batch file cap. A live
  barrel cannot point at a symbol removed in an earlier batch, and a type
  cannot outlive its same-file users. These are the Requo `batch_14` and
  `MessageComplexity` failures.
- The cleanup applier no longer removes the declaration that follows a
  removable symbol when the engine's exclusive byte span reaches past the
  symbol's inclusive `endLine`. It uses the reported line span and translates
  UTF-8 byte offsets before slicing source text, so non-ASCII files are edited
  safely too.
- A local `export { Name }` specifier is removed together with its declaration,
  and deleting the only specifier of a re-export statement removes the whole
  statement, so cleanup does not leave a dangling export or an empty
  `export { }` that fails typecheck. An import specifier that names a deleted
  symbol is removed from a file that is retained for its side effects, so the
  retained module does not fail typecheck on a missing export.
- The cleanup applier re-analyzes the restored tree before each batch. A file
  that contributes symbols to more than one batch (a barrel, or a declaration
  file linked by re-export edges) is edited with current spans instead of
  plan-time offsets, which is what rejected Requo `batch_16` with
  `invalid span for lib/ai/index.ts#QualityGateEvent`.
- The lazy TUI bundle now provides a Node `require` shim, so Ink's dependencies
  can load Node builtins at render time instead of falling back to the plain
  report.

## [1.0.1] — 2026-11-23

### Fixed

- The Action supplies the GitHub Actions bot git identity when the runner has
  none, so prune can commit its verified batch.
- `prune --pr` exits 4 with `gh`'s error when the pull request cannot be opened,
  instead of reporting success with no PR.

## [1.0.0] — 2026-11-23

The first Shrike release line, published to npm and GitHub. External launch
gates and release steps are recorded in [`docs/release.md`](docs/release.md).

### Added

- Whole-graph PR orphan delta: two snapshots, stable symbol IDs, witness paths,
  and attribution of code that became dead anywhere in the repository.
- Verified deletion batches: SCC-first cascade closure, deterministic batch
  partitioning, blast-radius caps, worktree apply, baseline/after checks,
  failure bisection, culprit revert, and per-batch commits.
- Content-addressed portable receipts with graph hashes, patches, check records,
  build evidence, policy, outcome, and rejected sets, plus Markdown PR rendering.
- Rust engine and TypeScript host with a versioned JSON protocol; `scan`,
  `delta`, `plan`, `proof`, `prune`, `hook`, `mcp`, `cache`, `recover`,
  and `version` commands.
- Next.js App Router and Pages Router depth: route entries, framework export
  surfaces, route-tree conflicts, segment configuration, server actions, RSC
  boundaries, and build-graph evidence.
- Advisory diagnostics for unused component props, absent optional props, enum
  members, class members, namespace members, Pages Router data keys, misplaced
  RSC directives, mixed client/server barrels, and client/server leak cones.
- Claude Code, Cursor, and OpenCode post-agent hooks with a 60-second fast path,
  baseline reuse, advisory check statuses, and a bounded `CleanupBrief`.
- GitHub Action with read-only `delta` and write-scoped `prune` modes,
  `fail-on: newly-dead`, artifact replacement/removal, fork refusal, and a
  committed bundle at `packages/action/dist/main.cjs`.
- Read-only MCP server exposing four tools over stdio.
- npm release path: unscoped `getshrike` package with `shrike` and `getshrike`
  bins, five `getshrike-*` platform packages, SHA-256 sidecars, a clean-machine
  install test, and a fail-closed launcher.
- Release workflow: five native targets, GitHub build-provenance attestations,
  `SHA256SUMS`, keyless Sigstore signing, GitHub release, npm provenance, and a
  registry smoke test.
- 315-case must-not-report corpus, 14 delta fixtures, 11 build-graph fixtures,
  5 prune e2e scenarios, and differential fixtures for knip and glob behavior.
- Paired competitor benchmark harness with 12 fixtures, cold/warm medians,
  finding-count validation, and PID-validated RSS sampling.

### Changed

- `findEngine()` now resolves `SHRIKE_ENGINE`, a repository
  `target/{release,debug}` build, the installed platform package, then `PATH`.
  Packaged binaries require and verify a `.sha256` sidecar.
- The GitHub Action resolves a project-local `getshrike` install, honors
  `SHRIKE_CLI`, and falls back to `shrike` on `PATH` instead of assuming a global
  binary.
- The published CLI is bundled with esbuild and has no runtime dependencies on
  the private `@shrike/*` workspace packages.
- GitHub Actions in CI, nightly, and release workflows are pinned by commit SHA.

### Fixed

- `prune --pr` strips a configured remote from the analyzed base ref before
  calling `gh pr create`, so the default `origin/main` opens a pull request
  against `main` instead of failing. It also checks the remote for an existing
  cleanup branch instead of trusting a stale remote-tracking ref.
- The GitHub Action resolves a project-local `getshrike` through its main entry
  when the installed package does not export `./package.json`.
- The release version setter is idempotent, so publishing the version already
  present in the source tree succeeds instead of failing the release.

### Security

- Keyless Sigstore signing for `SHA256SUMS`, npm provenance, GitHub
  attestations, and per-binary SHA-256 sidecars.
- No `postinstall` download of an unverified binary.
- Config and project checks run with allowlisted environments; receipts and logs
  redact repository roots, home directories, tokens, and declared secrets.
- Fork pull requests cannot run `prune` with a write token.

### Documentation

- README install path, package names, status, and repository map updated for the
  `getshrike` release.
- Launcher resolution order, tarball gates, supply-chain controls, and the
  `getshrike` naming decision are documented in
  [`docs/development.md`](docs/development.md) and
  [`docs/release.md`](docs/release.md).
- Competitor performance and accuracy reports for 2026-11-23.

### Known limitations

- No independent audit or labeled real-repository precision set exists yet.
- Receipt signing, reproducible release builds, and telemetry remain post-1.0.
- The Windows arm64 target and a standalone single-binary distribution are
  post-1.0; `win32-x64` is the only Windows platform package.

[1.1.0]: https://github.com/getshrike/shrike/releases/tag/v1.1.0
[1.0.1]: https://github.com/getshrike/shrike/releases/tag/v1.0.1
[1.0.0]: https://github.com/getshrike/shrike/releases/tag/v1.0.0
