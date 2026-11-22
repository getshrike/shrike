# Changelog

All notable changes to Shrike are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- The Action supplies the GitHub Actions bot git identity when the runner has
  none, so prune can commit its verified batch.
- `prune --pr` exits 4 with `gh`'s error when the pull request cannot be opened,
  instead of reporting success with no PR.

## [1.0.0] — 2026-11-23

The first Shrike release line, published to npm and GitHub. External launch
gates are tracked in [`TRACKER.md`](TRACKER.md).

### Added

- Whole-graph PR orphan delta: two snapshots, stable symbol IDs, witness paths,
  and attribution of code that became dead anywhere in the repository.
- Verified deletion batches: SCC-first cascade closure, deterministic batch
  partitioning, blast-radius caps, worktree apply, baseline/after checks,
  failure bisection, culprit revert, and per-batch commits.
- Content-addressed portable receipts with graph hashes, patches, check records,
  build evidence, policy, outcome, and rejected sets, plus Markdown PR rendering.
- Rust engine and TypeScript host with a versioned JSON protocol; `scan`,
  `delta`, `plan`, `proof`, `prune`, `apply`, `hook`, `mcp`, `cache`, `recover`,
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
- `docs/07` documents the actual launcher resolution order, tarball gates, and
  clean-machine install test.
- `docs/09` records the implemented supply-chain controls and the remaining
  external audit.
- `docs/12` records the `getshrike` naming decision as D-053.
- Competitor performance and accuracy reports for 2026-11-23.

### Known limitations

- No independent audit or labeled real-repository precision set exists yet.
- Receipt signing, reproducible release builds, and telemetry remain post-1.0.
- The Windows arm64 target and a standalone single-binary distribution are
  post-1.0; `win32-x64` is the only Windows platform package.

[1.0.0]: https://github.com/getshrike/shrike/releases/tag/v1.0.0
