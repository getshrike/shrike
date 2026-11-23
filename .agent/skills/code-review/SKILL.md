---
name: code-review
description: Review a local diff or pull request against Shrike's correctness, determinism, engine-host contract, fixture, and scope rules. Use when asked to review a change or a pull request.
---

# Review a change

## 1. Read the change

- `git diff main...HEAD` (or the PR diff), plus the commit titles.
- Read the linked `docs/` page for the area touched; behavior must match docs.

## 2. Check the invariants

Work through [the reviewer checklist](../../../docs/contributing.md), focusing on:

1. **Correctness** — liveness stays conservative; new reports have witnesses;
   new protections have a corpus case; no rule was loosened to pass.
2. **Determinism** — stable ordering, no timestamps, random IDs, or host paths
   in JSON artifacts.
3. **Contract** — `schemas/` and both runtimes agree; `ANALYSIS_VERSION`
   bumped when analysis output changed; no new engine/host coupling.
4. **Boundaries** — no new I/O in pure crates, no circular package imports,
   no weakened verification or receipt behavior.
5. **Scope and evidence** — one concern, no generated files or secrets,
   regression coverage added, gate output recorded in the PR.

## 3. Verify independently

- Run the gates that cover the diff (see `create-pr/references/gates.md`), not
  just the ones the author ran.
- For analysis changes, inspect the fixture or corpus case and confirm it would
  fail before the fix.

## 4. Report

- Order findings by severity and give `file:line` plus a concrete fix for each.
- Separate confirmed defects from plausible risks; say which gates you ran and
  their results.
- Approve only when evidence supports it. If blocked, name the blocker and who
  can decide.
