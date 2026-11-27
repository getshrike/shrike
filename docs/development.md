# Development setup and commands

## Prerequisites

- Node.js 22+ and pnpm 12.18.0 (`package.json` declares `packageManager`).
- A stable Rust toolchain; `rust-toolchain.toml` pins the version used in CI.
- `gh` is optional; `prune` and `clean` use it with `--pr` to open a draft PR.

## Bootstrap

```bash
pnpm install --frozen-lockfile
cargo build --release -p shrike-engine
pnpm run build:ts            # types, CLI bundle, committed Action bundle
```

The TypeScript host needs a native engine. It resolves one, in order, from
`SHRIKE_ENGINE`, a repository `target/{release,debug}/shrike-engine`, the
installed platform package, then `PATH`. A missing engine fails closed with exit
code 4.

## Day-to-day commands

Most commands below are root scripts in `package.json`; Rust and standalone
gates are invoked directly.

| Goal | Command |
| --- | --- |
| Typecheck all packages | `pnpm run typecheck` |
| Lint repository scripts/sources | `pnpm run lint` |
| Rust + Node test suites | `pnpm test` |
| Rust tests only | `cargo test --workspace` |
| Node tests only | `pnpm run test:node` |
| Format / lint Rust | `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings` |
| Full local gate suite | `node scripts/check.mjs` |
| Public skill validation | `node scripts/check-skills.mjs` |
| Docs and link gates | `node scripts/check-docs.mjs`, `node scripts/check-links.mjs` |
| Schema validation | `node scripts/check-schemas.mjs`, `node scripts/validate-instances.mjs` (also `pnpm run check:schemas`) |
| Performance harness | `pnpm run bench` |

See [testing.md](testing.md) for the fixture, corpus, differential, and e2e
gates, and [architecture.md](architecture.md) for what each part of the tree is
for.

## Running the tool from source

```bash
# Analyze the current repository
node packages/cli/src/main.ts scan --json

# Dead-weight report for the current tree (plain output)
node packages/cli/src/main.ts report

# Compare a branch against a base
node packages/cli/src/main.ts delta --base origin/main --json

# Preview and apply current-state cleanup through the verified pipeline
node packages/cli/src/main.ts clean --all --dry-run
node packages/cli/src/main.ts clean --all

# Verify a branch deletion plan in a worktree (`prune` is the alias)
node packages/cli/src/main.ts clean --base origin/main

# Explain one symbol, install the agent skill, add the advisory CI workflow
node packages/cli/src/main.ts why src/lib/format.ts#formatInvoice
node packages/cli/src/main.ts install --dry-run
node packages/cli/src/main.ts ci install
```

`clean` and `prune` work in a branch worktree under `.shrike/worktrees/`, write
receipts under `.shrike/receipts/`, and never commit to `main`. `--in-place` is
explicit opt-in. Any command that compares a base to head refuses a dirty
working tree unless `--allow-dirty` is set (the post-agent hook sets it
automatically).
Exit codes: `0`
clean, `1` findings, `2` usage/config, `3` verification failed, `4` could not
run, `5` revert failed.

The engine speaks newline-delimited JSON on stdin/stdout:

```bash
printf '%s\n' '{"protocolVersion":"1.0","id":"a","command":"analyze","payload":{"root":"fixtures/delta/removed-caller/base","config":{}}}' \
  | target/debug/shrike-engine
```

## Working with caches and generated files

- `.shrike/cache/` holds analysis snapshots keyed by source and config hashes.
  Delete it when a cache key should have changed but did not; changing analysis
  output must bump `ANALYSIS_VERSION` in `crates/shrike-engine/src/cache.rs`.
- Build outputs (`target/`, `packages/*/dist/`, `dist/`) and platform `bin/`
  files are not hand-edited. Regenerate them with `pnpm run build:ts` and
  `cargo build`; the CLI bundle includes a lazily loaded `dist/tui.js` for the
  interactive report and copies the canonical `skills/` tree to
  `packages/cli/dist/skills/` for the installer.
- `packages/action/dist/main.cjs` is committed; CI asserts it is current after
  `pnpm run build:ts`.

## Troubleshooting

| Symptom | Check |
| --- | --- |
| `could not run` (exit 4) | Engine path and build; `SHRIKE_ENGINE` override |
| Stale findings after an analysis change | Bump `ANALYSIS_VERSION`, clear `.shrike/cache/` |
| Typecheck fails only in CI | Node version and `pnpm install --frozen-lockfile` |
| Action bundle diff in CI | Run `pnpm run build:ts` and commit the regenerated bundle |
| Link gate fails | `node scripts/check-links.mjs` prints the offending Markdown link |
