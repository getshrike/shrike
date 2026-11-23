---
name: add-fixture
description: Add regression coverage in the correct Shrike layer (Rust/Node unit test, delta or verification fixture, must-not-report corpus case, or differential case) and run the matching gate. Use when fixing a bug, preventing a false positive, or covering new analysis behavior.
---

# Add regression coverage

## 1. Classify the behavior

- Pure function / protocol shape → Rust `#[test]` or `packages/*/*.test.mjs`.
- Orphan delta, attribution, cascade → `fixtures/delta/<scenario>/`.
- Verified deletion, baseline, revert → `fixtures/verification/`.
- Live code reported dead, framework entry, protected export → a case in
  `fixtures/must-not-report/cases/` plus `manifest.json`.
- Disagreement with a competitor tool → `fixtures/differential/` or
  `fixtures/differential-glob/`.

Read [docs/testing.md](../../../docs/testing.md) for the full layer table.

## 2. Build the case

- Make the fixture minimal, self-contained, and deterministic; no network.
- Register it in the relevant manifest with expected findings. For gated rules
  add both sides: protected where the rule applies and reportable where it does
  not.
- Include a witness or reason for each expected finding so the case can be
  debugged later.

## 3. Invalidate caches

- If analysis output changed, bump `ANALYSIS_VERSION` in
  `crates/shrike-engine/src/cache.rs` and clear `.shrike/cache/` locally.
- A schema change also requires updating `schemas/`, both runtimes, and the
  fixture contract gate.

## 4. Prove the regression

- Run the matching gate from `docs/testing.md` and confirm the new case passes.
- Confirm the case fails against the old behavior when practical.
- Run `node scripts/check.mjs` before proposing the change for review, and
  record the commands and output in the PR.
