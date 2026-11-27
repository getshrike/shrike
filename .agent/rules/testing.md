# Testing rule

Every behavior change ships with evidence, not a claim.

- Add the smallest regression case that would have failed before the change:
  a Rust/Node unit test, a `fixtures/delta/` scenario, a `fixtures/verification/`
  scenario, a `fixtures/must-not-report/cases/` case, or a differential case.
  See `docs/testing.md` for the mapping and commands.
- A false-positive fix belongs in the must-not-report corpus. Gated rules need
  a two-sided case: protected where the rule applies, reportable where it does
  not.
- Bump `ANALYSIS_VERSION` in `crates/shrike-engine/src/cache.rs` whenever
  analysis output changes; clear `.shrike/cache/` locally when validating.
- Run the gates relevant to the change, then the full local suite:
  `pnpm -r typecheck`, `pnpm run lint`, `pnpm test`, Rust fmt/clippy, and
  `node scripts/check.mjs`. Docs changes also run `check-docs.mjs` and
  `check-links.mjs` (Mintlify pages additionally use `mint validate` and
  `mint broken-links`).
- Paste the commands and their results into the PR. "Tests pass" without
  output is not evidence.
- Never skip, delete, or loosen a test to get green. If a gate is wrong, fix
  the gate in its own scoped change and explain why.
