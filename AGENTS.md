# AGENTS.md

Shrike is a polyglot monorepo: a Rust engine computes a reachability graph and
emits JSON; a TypeScript host runs commands, checks, and receipts. This file is
the short entry point. The linked docs are the source of truth — read the one
for the task you are doing before you edit code.

## Non-negotiables

- The engine and the host communicate only through `schemas/` and the JSON
  protocol. No Rust crate imports TypeScript, and no package imports Rust source.
- Analysis output is deterministic. Same input must produce byte-identical JSON;
  no timestamps, unordered sets, or host-specific paths in artifacts.
- Never commit directly to `main`. Work one concern per branch and use
  Conventional Commit titles (`feat:`, `fix:`, `docs:`, `test:`, `chore:`, ...).
- Never hand-edit generated output: `packages/action/dist/main.cjs`,
  `packages/*/dist/`, platform `bin/`, or `Cargo.lock` unless a dependency changed.
- A behavior change needs a test or fixture and a recorded gate run. Report
  commands and their output, not claims of success.

## Setup and verification

```bash
pnpm install
cargo build --release -p shrike-engine   # host commands need the native engine

pnpm run typecheck && pnpm run lint && pnpm test
cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings
node scripts/check.mjs                   # schemas, fixtures, corpus, e2e gates
node scripts/check-docs.mjs && node scripts/check-links.mjs
```

See the CI workflows in `.github/workflows/` for the authoritative gate list.

## Read next (progressive disclosure)

| Task | Doc |
| --- | --- |
| First day, commands, troubleshooting | [`docs/development.md`](docs/development.md) |
| What the product does, commands, integrations | [`docs/features.md`](docs/features.md) |
| Algorithms, pipeline, invariants, worked example | [`docs/how-it-works.md`](docs/how-it-works.md) |
| Where code lives, pipeline, where to change things | [`docs/architecture.md`](docs/architecture.md) |
| Writing and running tests, fixtures, performance | [`docs/testing.md`](docs/testing.md) |
| Commits, PRs, review, code quality | [`docs/contributing.md`](docs/contributing.md) |
| Tagging and publishing | [`docs/release.md`](docs/release.md) |
| Doc index | [`docs/README.md`](docs/README.md) |

## Agent rules and skills

`.agent/rules/` are always-on rules; `.agent/skills/` are on-demand workflows for
tasks such as creating a PR or reviewing code. `opencode.jsonc` wires both into
OpenCode. See [`.agent/README.md`](.agent/README.md) for the layout.
