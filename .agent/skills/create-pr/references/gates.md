# Gate mapping

Pick the smallest set that covers the change, then run the full local suite
when the change is ready for review.

| Changed area | Required checks |
| --- | --- |
| Rust crate | `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` |
| TypeScript package | `pnpm run typecheck`, `pnpm run lint`, `pnpm run test:node` |
| Analysis behavior | above + `node scripts/check.mjs` + corpus/differential gates in `docs/testing.md`; bump `ANALYSIS_VERSION` |
| Schema or protocol | `node scripts/validate-schemas.mjs`, `node scripts/check-schemas.mjs`, both runtimes' tests, updated fixtures |
| Fixtures/corpus | matching gate from `docs/testing.md` (`run-fixtures`, `corpus-runner`, `differential`, `e2e-prune`) |
| Docs, rules, skills | `node scripts/check-docs.mjs`, `node scripts/check-links.mjs` |
| Action or packaging | `pnpm run build:ts`, `node scripts/check-package.mjs`, confirm committed bundle diff is clean |

Full local suite: `pnpm run typecheck && pnpm run lint && pnpm test`, Rust
fmt/clippy, `node scripts/check.mjs`, then the docs and link gates. CI adds
audits, locked installs, clean-machine installs, and the determinism check; see
`.github/workflows/ci.yml`.
