# Testing, fixtures, and gates

## Test layers

| Layer | Command | What it covers |
| --- | --- | --- |
| Rust unit/integration | `cargo test --workspace` | Parsers, resolver, graph, delta, plan, engine |
| Node tests | `pnpm run test:node` | CLI contracts, verifier, receipts, agent, action |
| Both | `pnpm test` | Runs the two above in sequence |
| Fixture matrix | `node scripts/run-fixtures.mjs --engine` | Delta, verification, build, build-graph fixtures |
| Fixture contract | `node scripts/verify-fixtures.mjs` | Fixture manifests and expected outputs |
| End-to-end prune | `node scripts/e2e-prune.mjs` | Apply, verify, revert, receipt on real scenarios |
| End-to-end clean --all | `node scripts/e2e-clean-all.mjs` | Dry-run, current-state deletion, receipt, protected/live code |
| Corpus | `node scripts/check-corpus.mjs`, `node scripts/corpus-runner.mjs` | Must-not-report false-positive corpus |
| Differential | `node scripts/differential.mjs` (add `--fixture fixtures/differential-glob`) | Agreement with competitor tools |
| Public skill | `node scripts/check-skills.mjs` | Frontmatter, links, command allowlist, budgets, LF, and source/dist parity |
| Skill evals | `evals/skills/shrike/` | Manual trigger/safety matrix; the deterministic report/delta smoke test runs in `scripts/test-install.mjs` |
| Everything local | `node scripts/check.mjs` | Schemas, receipts, package, corpus, fixtures, e2e, skills |

CI (`.github/workflows/ci.yml`) adds `cargo fmt --check`, clippy with
`-D warnings`, `cargo audit`, `cargo deny check`, `cargo check --workspace` on
macOS and Windows, `pnpm audit --prod`, `pnpm -r typecheck`, locked installs,
clean-machine package installs, the `node scripts/check.mjs` gate suite (run
in the node job), the fixture contract gate
(`node scripts/verify-fixtures.mjs`), schema/fixture/corpus/e2e-prune gates,
and a byte-identical determinism check. Run the local equivalents before
opening a PR.

## Adding regression coverage

Choose the smallest layer that pins the behavior:

- **False positive (live code reported dead):** add a case under
  `fixtures/must-not-report/cases/<name>/` and register it in
  `fixtures/must-not-report/manifest.json`. Run `node scripts/corpus-runner.mjs`
  and `node scripts/check-corpus.mjs`. A case must assert both sides when the
  rule is gated (protected on one side, reportable on the other).
- **Delta or reachability behavior:** add a scenario under `fixtures/delta/`
  with its manifest and expected output, then run
  `node scripts/run-fixtures.mjs --engine`.
- **Verification behavior:** add a scenario under `fixtures/verification/` and
  run `node scripts/e2e-prune.mjs`; for current-state cleanup, also run
  `node scripts/e2e-clean-all.mjs`.
- **Competitor disagreement:** add to `fixtures/differential/` (or the glob
  variant) and run `node scripts/differential.mjs`.
- **Pure function or protocol bug:** a Rust `#[test]` or a `*.test.mjs` beside
  the package source is enough.
- **Public skill behavior:** edit `skills/shrike/`, then run
  `node scripts/check-skills.mjs` and the install tests. A new shipped skill
  needs a malformed-case in the validator self-test and an eval entry.

Rules of thumb:

1. Every bug fix gets a regression case that fails before the fix.
2. Bump `ANALYSIS_VERSION` in `crates/shrike-engine/src/cache.rs` when analysis
   output changes, then clear `.shrike/cache/` locally.
3. Keep fixtures deterministic and self-contained; no network at test time.
4. A schema change requires updated fixtures plus `node scripts/check-schemas.mjs`
   (also available as `pnpm run check:schemas`) and
   `node scripts/validate-instances.mjs`. The first checks that schema files and
   fixture JSON parse; the second validates real engine output, the default
   config, receipts, and the cleanup-report fixtures under `fixtures/report/`
   against the cleanup-report schema, and includes a negative self-test. Both
   run in `node scripts/check.mjs` and in the `fixtures` CI job.

## Determinism

Identical input must produce byte-identical JSON across runs and thread counts.
CI pins this with two `analyze` calls on the same fixture, normalized only by
their request `id`, and `diff`s the results. Do not introduce timestamps, random
IDs, iteration order over hash maps, or absolute host paths into artifacts.

## Performance

The benchmark harness generates deterministic synthetic repositories:

```bash
cargo build --release -p shrike-engine
pnpm run bench
node benchmarks/harness.mjs --files 10000 --runs 7
```

Budgets: 1,000 files ≤ 1.5 s cold / ≤ 400 ms warm; 10,000 files ≤ 6 s cold /
≤ 600 MB peak RSS; a 1,000-file warm delta ≤ 1 s. Benchmarks are
environment-specific — report the machine and commit with any measurement, and
never treat a local number as a CI gate.
