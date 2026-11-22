# 10 — Non-Goals and Risks

> **Parent:** [MASTER_PROMPT.md](../MASTER_PROMPT.md) · **Tracker:** [TRACKER.md](../TRACKER.md)
> **Status:** canonical · Authored 2026-11-20

---

## 1. Non-goals

A non-goal is not a "later." It is a decision that the product will not do this,
because doing it would weaken the wedge.

### 1.1 Another bundle optimizer

Vercel ships a Next.js bundle optimizer and an agent that uses the Turbopack graph
to cut client JavaScript. Competing on "reduce bundle size" is walking into the
vendor's guns.

**We do:** consume the build graph as evidence for deletion safety and byte
attribution.

**We do not:** suggest code splitting, lazy loading, RSC migration, or dependency
deduplication.

### 1.2 A 100+ plugin catalogue

Knip and Fallow own framework breadth. Every plugin is a maintenance liability
and a false-positive surface. A plugin catalogue is a race against teams with
more free labor.

**We do:** Next.js and React, deeply.

**We do not:** support 100 frameworks, 16 languages, or every build tool.

### 1.3 Multi-language analysis

aislop and fossil-mcp claim multi-language coverage. Verification is
framework-specific: a Python deletion proof does not help a Next.js app, and a
shallow adapter for ten languages produces ten ways to be wrong.

**We do:** TypeScript and JavaScript first, with JSX and TSX.

**We do not:** Python, Go, Rust, Java, or PHP in v1.

### 1.4 A slop score

A score is the wrong metric. It commoditizes the product into a linter, invites
gaming, and cannot be verified. "Cleanliness" is not the claim; "this deletion
did not break the build" is.

**We do:** report verified deletions, rejected batches, and evidence.

**We do not:** report a 0–100 score, a badge, or a leaderboard.

### 1.5 A type checker

The project already has a type checker. Shrike runs it as a check; it does not
reimplement it.

**We do:** consume typecheck and build results as evidence.

**We do not:** type-check, infer types, or replace `tsc`.

### 1.6 A generic MCP dead-code scanner

An MCP server is a distribution surface. A generic scanner with no verifier is
the crowded, undifferentiated product.

**We do:** expose four read-only tools over the CLI.

**We do not:** make MCP the product, host a scanner service, or accept repository
content as instructions.

### 1.7 Auto-commit to main

Trust is earned in review. Auto-commit is how a cleanup tool becomes an incident.

**We do:** open a draft PR with one commit per verified batch.

**We do not:** push to `main`, force-push a human branch, or mark a PR ready
automatically.

### 1.8 A hosted dashboard in v1

A dashboard is a second product: accounts, storage, retention, tenancy, and a
new trust boundary for source code. It is not needed to prove the wedge.

**We do:** local receipts, CI artifacts, and PR comments.

**We do not:** upload source, store repositories, or build a web app in v1.

### 1.9 A runtime tracer

Runtime coverage is useful evidence, but it requires running the app, instrumenting
it, and handling production data. It is a different product with a different
trust model.

**We do:** static evidence plus the project's own tests and build.

**We do not:** instrument runtime execution in v1.

### 1.10 More rules before the loop is proven

The temptation is to add rules when the product feels thin. The wedge is the
verified deletion loop, not rule count.

**We do:** deepen Next.js/React correctness and the verifier.

**We do not:** add rules that are not covered by the corpus and the verifier.

### 1.11 A service business

Agencies are a beachhead, not the identity. Shrike is a tool that produces a
verified PR, not a done-for-you cleanup service.

**We do:** provide a design-partner kit and receipts an agency can share.

**We do not:** sell manual cleanup in v1.

---

## 2. Risks

### 2.1 Incumbent ships verified deletion

**Likelihood:** medium · **Impact:** high

CodeTruth and Necro already provide deletion-safety evidence to agents. Fallow
has auto-fix and an audit mode. If one of them adds baseline-aware verification,
batch bisection, and receipts, the window narrows.

**Response:** go deeper on the whole-graph orphan delta, the build-graph join,
the precision corpus, and the receipt. A verdict is copyable; an operations
product with a track record is not.

**Early warning:** a competitor announcement that mentions "verified deletion,"
"receipt," or "bisect."

### 2.2 Verification is too slow for CI

**Likelihood:** high · **Impact:** high

A full typecheck, lint, test, and build can take 20 minutes on a large app.
A cleanup PR that takes 45 minutes will not run on every PR.

**Response:** tiered checks; fast path for hooks; standard path for local;
full path for CI and release branches; cache the baseline; add affected-only
selection after measuring. If the full path exceeds 15 minutes on a typical app,
the tiered path becomes the product.

### 2.3 Next.js build snapshot changes or disappears

**Likelihood:** medium · **Impact:** medium

`next analyze export` is young. Its schema can change, or the command can be
renamed.

**Response:** optional evidence, version guard, streaming parser with additive
tolerance, static fallback. The product still works without it.

### 2.4 Orphan delta is noisy

**Likelihood:** medium · **Impact:** high

Generated code, test-only entries, monorepo boundaries, and barrel files can
produce false or uninteresting deltas.

**Response:** corpus, conservative unknown edges, workspace awareness, entry
policy, public API blocking, and design-partner triage. If the delta is mostly
noise after triage, rework the abstraction before scaling.

### 2.5 Engine work delays the wedge

**Likelihood:** high · **Impact:** medium

Rust parsing and resolution are deep work. The product loop (delta → verify →
receipt) is the reason anyone cares.

**Response:** contract and fixtures first; timebox the parser; ship delta and
verification on a small fixture set before optimizing. Cut engine scope before
cutting the loop.

### 2.6 Buyer mismatch

**Likelihood:** medium · **Impact:** high

"Vibe coding cleanup" attracts non-technical founders who want a service. They
will not install a CLI.

**Response:** target teams with CI; use "vibe coding" in content only; measure
qualified sign-ups, not raw traffic.

### 2.7 Dirty tree or failed revert

**Likelihood:** low · **Impact:** high

A failed revert leaves the user's tree in an unknown state.

**Response:** worktree default; snapshot before in-place; exit 5 with a recovery
path; a failure drill in CI; never continue after a partial apply.

### 2.8 Flaky tests misclassified

**Likelihood:** high · **Impact:** medium

A flaky test can look like a regression caused by the deletion.

**Response:** rerun policy; failure signatures; classify `flaky`; record the
reruns; never claim a regression from a single unstable run.

### 2.9 False positive on a framework convention

**Likelihood:** medium · **Impact:** high

A route, server action, or dynamic import that the adapter misses is a release
blocker.

**Response:** fixture-first; the must-not-report corpus; build evidence;
conservative server-action handling; a public false-positive process.

### 2.10 Package name unavailable

**Likelihood:** medium · **Impact:** low

`shrike` may be taken on npm.

**Response:** check early; use a scope (`@shrike/cli`) or a different product
name; the CLI binary can remain `shrike`.

### 2.11 Running project scripts is a trust boundary

**Likelihood:** certain · **Impact:** medium

The verifier runs the repository's own scripts.

**Response:** document it; worktree; explicit consent; no secrets by default;
refuse fork PRs with write tokens.

### 2.12 Rust binary distribution complexity

**Likelihood:** medium · **Impact:** medium

Five platforms, glibc/musl, signing, checksums, and npm optional dependencies.

**Response:** start with four primary targets; publish a standalone binary;
verify checksums in the launcher; fail closed with a clear message.

### 2.13 `oxc` API churn

**Likelihood:** medium · **Impact:** medium

`oxc` is fast-moving. An API change can break the parser crate.

**Response:** pin the version; isolate it behind `shrike-syntax`; upgrade on a
schedule with the fixture suite as the gate.

### 2.14 Blast-radius cap suppresses useful deletions

**Likelihood:** medium · **Impact:** low

A large legitimate cleanup may exceed the default cap.

**Response:** configurable cap; explicit `--allow-oversized`; report the
suppressed batch; revisit the default with design-partner data.

### 2.15 The receipt becomes a compliance ritual

**Likelihood:** medium · **Impact:** medium

A receipt that nobody reads is theater.

**Response:** keep it short, link the exact checks and patches, render a human
summary in the PR, and make the machine JSON the artifact. A receipt without a
complete check record is not `verified`.

### 2.16 The agent hook is ignored

**Likelihood:** medium · **Impact:** medium

A hook that prints a wall of text is noise.

**Response:** one screen, top three items, a single next command; cap the brief;
never block the agent.

---

## 3. Falsification tests

These are the experiments that would change the plan:

1. **Buyer test.** Offer ten Next.js teams a verified deletion PR versus a list
   of findings. If they prefer the list, the wedge is wrong.
2. **Delta value test.** Run the delta on 50 real PRs. If fewer than 20% have a
   genuine newly dead file after triage, the abstraction is too noisy.
3. **Verification cost test.** Run the full path on five real apps. If the median
   exceeds 15 minutes, make tiering the product.
4. **Precision test.** Label 1,000 findings. If precision is below 95%, do not
   enable auto-deletion; fix the rules first.
5. **Hook test.** Install the hook for ten developers. If fewer than half run it
   again after a week, the hook is not the distribution surface.
6. **Competitor test.** If an incumbent ships verified deletion with receipts and
   a build join, move to the agency workflow or the build-attribution niche.
7. **Engine test.** If the Rust engine cannot hit the budgets after two documented
   optimization passes, reduce scope rather than shipping a slow engine.

---

## 4. Kill criteria

Shrike should stop or pivot if:

- the orphan delta cannot reach a useful signal-to-noise ratio after two
  abstraction revisions;
- verification cannot run on real apps within a usable time, even tiered;
- design partners consistently prefer a report or a service to a verified PR;
- a well-funded incumbent makes verified deletion free, reliable, and receipted
  before Shrike has distribution;
- the precision corpus cannot reach 95% on the rules that matter.

Stopping is cheaper than shipping a tool that breaks builds.

---

## 5. Open questions

| ID | Question | Owner |
|---|---|---|
| Q-047 | What is the maximum acceptable full-path time before tiering becomes the product? | PLAN-6 |
| Q-048 | Which non-goal is most likely to be forced by a design partner? | PLAN-5 |
| Q-049 | Is the "no dashboard" decision reversible after 1.0? | PLAN-6 |
