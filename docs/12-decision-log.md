# 12 — Decision Log

> **Parent:** [MASTER_PROMPT.md](../MASTER_PROMPT.md) · **Tracker:** [TRACKER.md](../TRACKER.md)
> **Status:** canonical · Authored 2026-11-20

This log records the decisions that shape Shrike 1.0, the alternatives that were
rejected, and the conditions under which a decision would be revisited. The
tracker holds the one-line summary; this document holds the reasoning.

---

## D-001 — Shrike 1.0 is a greenfield application

**Decision:** Build Shrike 1.0 from an empty repository. No source file, schema,
fixture, cache format, or package name from a prior prototype is a dependency.

**Alternatives considered:**

- Upgrade the prior prototype in place.
- Rewrite the prototype module by module.

**Why:** The prior prototype carried an in-place upgrade plan, a TypeScript
engine, a rule catalogue, and a config surface that pull the product toward
being a linter. A greenfield start makes the folder structure, the protocol, and
the Rust engine first-class. It also removes the temptation to preserve a cache
format or a command quirk that no user depends on.

**Consequences:**

- No migration path; there are no users to migrate.
- Prior lessons are inputs, not code.
- The package is documentation-first; implementation starts at PLAN-0.

**Revisit if:** the repository gains users before 1.0 and a breaking reset would
strand them.

---

## D-002 — Rust engine, TypeScript host

**Decision:** The analysis hot path is Rust. The CLI, config execution, verifier,
receipts, hooks, Action, and MCP wrapper are TypeScript.

**Alternatives considered:**

- TypeScript-only engine.
- Pure Rust CLI.
- Rust core with an FFI verifier.
- WASM engine inside Node.
- A TypeScript semantic service.

**Why:**

- Two snapshots, reachability diff, SCC condensation, and cascade closures are
  memory-heavy. Retaining JavaScript ASTs blows the 600 MB budget; arena-allocated
  Rust summaries do not.
- `shrike.config.ts` is executable TypeScript and the target toolchain is Node.
- The verifier is process orchestration, which is what Node is good at.
- Determinism is easier with interned IDs and sorted arenas.
- A native binary avoids Node startup on the hook path.

**Consequences:**

- Two toolchains, two CI lanes, and a protocol boundary.
- The engine must be distributed as native binaries.
- The host can change without recompiling the engine.

**Revisit if:** the engine cannot hit budgets after two documented optimization
passes, or if native distribution proves impossible on the design partners' CI.

---

## D-003 — The whole-graph PR orphan delta is the wedge

**Decision:** The primary signal is code that was live at the merge base and is
newly dead at the PR head, anywhere in the repository.

**Alternatives considered:**

- Current-state dead-code scan (Knip, Fallow).
- Changed-file audit (Fallow audit, Knip CI guidance).
- A ranked list of all dead code.
- A cleanliness score.

**Why:** A PR can kill code without touching it. Changed-file scoping hides the
most valuable finding. Current-state scans cannot attribute the change that
caused it. A score is not actionable.

**Consequences:**

- Two snapshots per delta.
- Stable IDs across commits.
- Witnesses at base, not just live/dead booleans.
- Comparability checks; an incomparable pair is an error, not a guess.

**Revisit if:** the delta is mostly noise on real PRs after two abstraction
revisions.

---

## D-004 — Verified deletion batches with bisection

**Decision:** Apply each closure-complete batch in isolation, verify against a
baseline, bisect failures, revert only the culprit, and keep verified batches.

**Alternatives considered:**

- Advisory verdicts only.
- One finding per patch.
- Apply everything, run checks once, revert everything on failure.
- Auto-fix without verification.

**Why:** One failure should not discard the whole cleanup. Bisection isolates
the regression and keeps the rest. Independent batches make the PR reviewable
and the failure recoverable.

**Consequences:**

- The verifier is an operations product with real failure modes.
- Batches must not share files or dependency order.
- Check cost multiplies with batch count; hence the batch cap.

**Revisit if:** verification cost makes per-batch checks impractical, in which
case a single apply with bisection is the fallback.

---

## D-005 — The portable receipt is the product artifact

**Decision:** Every batch, including rejected ones, emits a content-addressed
JSON receipt with graph hashes, patches, checks, baseline status, build evidence,
policy, and outcome.

**Alternatives considered:**

- A score.
- A dashboard.
- A plain report.
- A PR comment only.

**Why:** A receipt is auditable, shareable, and reproducible. It is the unit of
trust for a reviewer and the artifact for CI. It is not a metric that can be
gamed.

**Consequences:**

- The receipt schema is part of the protocol contract.
- A receipt without a complete check record cannot be `verified`.
- The receipt ID is deterministic except for `createdAt`.

**Revisit if:** design partners never open the receipt; then the PR summary is
the product and the receipt is an artifact.

---

## D-006 — Build graph is optional evidence

**Decision:** Consume `next analyze export` when available. Never require it.
A `present` verdict suppresses deletion; an `absent` verdict confirms but does
not authorize.

**Alternatives considered:**

- Hard dependency on the Turbopack graph.
- Ignore the build graph.
- Run `next build` on every scan.

**Why:** The build graph adds confirmation and byte attribution, but it is young
and versioned. A hard dependency would make Shrike fragile for 15.x and for apps
with custom builds. Optional evidence keeps the static product usable.

**Consequences:**

- A fallback matrix and a status field in the receipt.
- A version guard and a streaming parser.
- `--require-build-evidence` for teams that want the strict path.

**Revisit if:** the export becomes stable and universal, at which point it can
become a default-on input.

---

## D-007 — Next.js and React depth, not breadth

**Decision:** Support Next.js App Router and Pages Router deeply, plus React
roots and package entry points. Do not build a 100+ plugin catalogue.

**Alternatives considered:**

- Match Knip's plugin breadth.
- Multi-language support.
- Generic file-convention heuristics.

**Why:** Breadth is owned by teams with more free labor. A missed Next.js
convention is a false positive, and false positives are release blockers. Depth
is the moat.

**Consequences:**

- Framework conventions are fixture-tested.
- The must-not-report corpus is framework-heavy.
- Other frameworks are out of scope, not "later."

**Revisit if:** design partners are blocked by a second framework (for example,
Vite + React Router) and the abstraction generalizes cleanly.

---

## D-008 — No slop score

**Decision:** Do not produce a 0–100 score, badge, or leaderboard.

**Alternatives considered:**

- A cleanliness score.
- A trend line.
- A repo grade.

**Why:** A score is uncalibrated, gameable, and the wrong metric. It turns the
product into a linter and invites "my score went down" support. The receipt is a
bounded, checkable claim.

**Consequences:**

- Marketing must lead with the verified PR and the receipt.
- Competitors with a score will look simpler; that is acceptable.
- Precision is published per rule instead of a single number.

**Revisit if:** never for the core product; a score may exist as a separate,
clearly labeled aggregate in a future report.

---

## D-009 — PR first, never auto-commit

**Decision:** Verified changes land on a `shrike/cleanup-*` branch as a draft PR
with one commit per batch. Never push to `main`.

**Alternatives considered:**

- Auto-commit to `main`.
- Commit to the current branch.
- A patch file only.

**Why:** Trust is earned in review. Auto-commit turns a bug into an incident and
makes the tool impossible to adopt in a protected repository.

**Consequences:**

- The PR flow is part of the product.
- Branch protection and fork rules matter.
- The user can still apply locally with `--in-place` and review the diff.

**Revisit if:** a design partner explicitly wants a non-draft PR; still never
`main`.

---

## D-010 — Fail closed on unknown edges

**Decision:** An unresolved dynamic edge, an unparsed file, a missing check, an
unavailable engine, or an incomparable snapshot blocks deletion.

**Alternatives considered:**

- Best-effort deletion with a warning.
- Confidence-based deletion.
- Silently ignoring unknown edges.

**Why:** One false positive on a route or a server action teaches the team to
ignore the tool. Fail-closed is the product promise.

**Consequences:**

- Recall is lower than a permissive tool.
- Blocked findings must be explained, not hidden.
- The proof trace records the unknown edge.

**Revisit if:** blocked findings dominate on real repos and a bounded,
well-understood exception (for example, test-only scope) can be safe.

---

## D-011 — npm CLI with platform binary packages

**Decision:** Ship `shrike` on npm with a small TypeScript launcher and optional
platform packages containing the native engine. Also publish a standalone
binary.

**Alternatives considered:**

- A single static binary only.
- WASM.
- A container image only.
- A hosted service.

**Why:** The buyer already uses npm and Node. Optional platform packages are the
standard pattern for native CLIs and avoid a postinstall download. A standalone
binary covers non-Node users.

**Consequences:**

- A release matrix for Linux x64/arm64, macOS x64/arm64, and Windows x64.
- Checksums and signing.
- A clear failure if the platform package is missing.

**Revisit if:** native distribution fails on a design partner's CI; then evaluate
WASM or a hosted analyzer.

---

## D-012 — Deterministic, content-addressed outputs

**Decision:** Identical input produces byte-identical snapshots and plans.
Receipt IDs are content hashes excluding `createdAt`.

**Alternatives considered:**

- Best-effort ordering.
- Timestamp-based IDs.
- Random UUIDs.

**Why:** Determinism makes CI reproducible, receipts verifiable, and diffs
reviewable. It also exposes nondeterminism bugs early.

**Consequences:**

- Sorted arenas and stable IDs.
- No hash-map iteration in output.
- Parallel results merged in file order.
- Tests at 1, 2, and 8 threads.

**Revisit if:** a nondeterministic input (for example, a generated file) cannot
be normalized; then the snapshot records the input hash and the limitation.

---

## D-013 — Blast-radius guard

**Decision:** A batch defaults to ≤ 50 symbols, ≤ 10 files, and ≤ 5% of the live
graph. Oversized batches are split or refused; an override is explicit and
recorded.

**Alternatives considered:**

- Unbounded batches.
- A fixed file limit only.
- A user-defined cap only.

**Why:** A reviewer will not accept a 1,000-file surprise. A bounded batch keeps
the PR reviewable and the bisection cost sane.

**Consequences:**

- A `BlastRadiusReport` in the plan.
- A split algorithm that respects SCCs and files.
- A receipt field for overrides.

**Revisit if:** the cap suppresses legitimate cleanups repeatedly; raise the
default with data, not by removing the guard.

---

## D-014 — Agent hook and MCP are distribution, not identity

**Decision:** Ship a post-agent hook, a GitHub Action, and a read-only MCP
wrapper. The verified PR remains the product.

**Alternatives considered:**

- MCP-first product.
- A chat agent.
- A dashboard-first product.

**Why:** The hook meets the developer in the loop; the PR is where the change is
reviewed; MCP is how agents ask for evidence. None of them replaces the verifier.

**Consequences:**

- The hook never applies a deletion.
- MCP exposes four read-only tools.
- The Action has a read-only delta mode.

**Revisit if:** agents become the primary reviewer and a mutation-capable MCP
tool is safe; even then, it opens a PR, not a commit.

---

## D-015 — TypeScript and JavaScript only

**Decision:** Support `.ts`, `.tsx`, `.js`, `.jsx`, `.mjs`, `.cjs`, and
declaration files. No other languages in v1.

**Alternatives considered:**

- Multi-language via tree-sitter.
- Python first.
- Framework-specific adapters for Vite, Svelte, Vue.

**Why:** Verification is framework-specific. A shallow adapter for ten languages
produces ten false-positive surfaces. The buyer is a Next.js team.

**Consequences:**

- The parser is `oxc`.
- The corpus is TS/JS.
- Other languages are a separate product decision.

**Revisit if:** a design partner's monorepo has a Python service that the
cleanup must cross; even then, scope it as a separate adapter.

---

## D-016 — No hosted dashboard in v1

**Decision:** Local receipts, CI artifacts, and PR comments. No web app, no
accounts, no source upload.

**Alternatives considered:**

- A hosted dashboard.
- A team platform.
- A SaaS scanner.

**Why:** A dashboard is a second product with a new trust boundary. It is not
needed to prove the wedge. Local-first is also a selling point.

**Consequences:**

- No telemetry by default.
- Receipts are files.
- Sharing is via the PR or an artifact link.

**Revisit if:** design partners ask for cross-repo history and accept a
source-free metadata upload; that is a post-1.0 product.

---

## D-017 — SCC-first cascade and deterministic batches

**Decision:** Condense cycles with Tarjan before computing the cascade closure,
and partition batches with a deterministic greedy coloring of the conflict graph.

**Alternatives considered:**

- Per-symbol closure without SCCs.
- Recursive deletion.
- Exact graph coloring.

**Why:** A cycle is a unit. Deleting one node can leave the rest live and produce
an unstable plan. Exact coloring is NP-hard and unnecessary; greedy is near-
optimal on file/dependency conflict graphs and deterministic.

**Consequences:**

- No partial SCC in a plan.
- Batch order is stable.
- Bisection respects SCCs.

**Revisit if:** real conflict graphs produce pathological color counts; then add
a better heuristic, not exact coloring.

---

## D-018 — Fail closed on protocol errors

**Decision:** A malformed engine response, a timeout, a crash, or a missing field
is a protocol error: exit 4, no deletion.

**Alternatives considered:**

- Retry and continue.
- Best-effort parse.
- Treat a missing field as empty.

**Why:** An omitted `unknown` edge is not "no unknown edges." A partial result is
not a proof.

**Consequences:**

- Contract tests for malformed, timeout, crash, and partial results.
- The host reads structured output, not just the exit code.
- The engine and host versions are checked.

**Revisit if:** a transient error is provably safe to retry; the retry must still
produce a complete result.

---

## D-019 — Server actions are always entries

**Decision:** Every export of a module-level `"use server"` file is an entry,
regardless of `clientBoundaryPolicy`. The policy only controls `"use client"`
seeding.

**Alternatives considered:**

- Keep server-action exports gated by `protect`.
- Report unimported actions as unused with a warning, as fallow does.

**Why:** Next.js registers every export of a `"use server"` module in the
server-reference manifest once the module is reached, whether or not application
code imports it. Actions are invoked through serialized references, so the
import graph cannot prove such an export unused.

**Consequences:**

- The default `graph` policy no longer reports server-action exports.
- `protect` still seeds `"use client"` boundaries and the server-action module.
- Accounteer: 23 server-action candidates removed; fallow reports the same 23
  as `unused-server-action` warnings, not deletions.

**Revisit if:** a build manifest proves an action is absent from every client
reference and no external client can invoke it. Even then, absence is not proof
of death.

---

## D-020 — Server actions follow the graph by default

**Decision:** `serverActionPolicy` defaults to `graph`. A `"use server"` export
is an ordinary export: an import, a JSX `action={fn}` binding, a call, or any
other project reference keeps it live, and an action with no project reference
is a deletion candidate. `serverActionPolicy: "protect"` preserves D-019 by
seeding every action and the module as entries.

**Alternatives considered:**

- Keep D-019's unconditional protection. Knip 5.88.1 and fallow 3.45.0 both
  report the unimported actions on requo (25) and accounteer (23).
- Report unimported actions only as warnings, as fallow does. Shrike has no
  warning bucket in the snapshot, and a warning is not actionable in prune.

**Why:** Next.js registers an action id once a `"use server"` module is reached,
but the id is only embedded in code that references the action. An unreferenced
action is not reachable from the project's UI; it is dead code that remains
POST-able only if its id leaks. Both competitors treat that as a cleanup
finding, and Shrike's verified-deletion flow can still use client-reference
build evidence to keep a referenced action live. Teams that treat a leaked id as
a supported entry point can opt into `protect`.

**Consequences:**

- Requo: 563 → 593 candidates: the 25 actions both competitors report plus five
  unreferenced schemas/helpers exported from the same `"use server"` modules.
- Accounteer: 130 → 153 candidates, restoring the 23 D-019 removed.
- Next-portfolio is unchanged at 8 candidates.
- The default corpus case now requires the unimported action to be reported;
  `protect` has a dedicated graph test.

**Revisit if:** a real Next.js application invokes an action without any static
reference (for example through a hand-authored action id or an external client
contract).

---

## D-021 — Test and script modules are entries; their exports follow the graph

**Decision:** A test file or standalone script is an entry because a runner or
the repository executes the module, not because another module imports its
exports. Ordinary test exports and script exports are candidates unless
something references them. Three export surfaces stay seeded because a tool
consumes them directly: Storybook/Ladle stories, Jest `__mocks__` modules, and
type tests (`.test-d.*` files and `__typetest__/` modules).

**Alternatives considered:**

- Seed every export of every test and script module, the previous behavior.
  Knip 5.88.1 and fallow 3.45.0 both report requo's unused e2e constants and
  accounteer's unused fixture modules; whole-module protection hid them.
- Stop treating ordinary test modules as entries. Their imports would then be
  dead, so every test's support code would be reported; the runner does execute
  the file, so the module entry is correct.
- Seed test exports but not script exports. The two are executed the same way,
  and neither competitor protects either surface.

**Why:** Vitest, Jest, and node:test discover and execute the test file; they do
not read arbitrary named exports. A helper exported from a test file is ordinary
code and must be imported to stay live. Storybook renders every story export,
Jest loads `__mocks__` by convention, and type tests exist to export assertions,
so those surfaces remain entries. Scripts are the same case in production: the
runtime executes the module, while its exports are helpers.

Measured on the current requo working tree with one snapshot and both engines
compared from a cleared cache: 648 → 669 candidates, exactly +21 and 0 removals.
The 21 are 5 Knip-only, 3 reported by both competitors, and 13 with no
competitor agreement: ten declarations in an unreferenced
`tests/support/mock-model.ts` and three unused builders/anchors in
`tests/support/fixtures/polar.ts`. A namespace, spread, or dynamic reference
would have kept them live; none exists. Accounteer is 153 → 161 (+8, 0
removals), all unimported fixtures, with Knip agreeing on
`tests/fixtures/paymongo.ts#unknownOrderEvent`. Next-portfolio is unchanged at
8.

**Consequences:**

- Test and script modules remain module entries, so their imports and top-level
  work stay live.
- The corpus case `test-entry-unused-export` pins an unused test helper as
  reportable while the imported helper, story exports, and mock exports stay
  live.
- The graph test also pins `.test-d.*` and `__typetest__/` exports as entries.

**Revisit if:** a test runner or bundler starts consuming named exports from
ordinary test files, or a real repository reaches test helpers dynamically.

---

## D-022 — Unused-file findings require no entry, no import, and a dead module

**Decision:** A file is an unused-file finding only when it parsed successfully,
is neither public API nor generated, no entry names it, no other file has an
incoming edge to any of its symbols, and its module symbol is not live. This is
an import-level rule, not "all declarations are candidates": a barrel imported
by a live module is used even when every export it forwards is dead, and a file
imported only by dead code waits for the deletion cascade. A side-effect-only
module has no symbol the symbol report can name, so the file list is the only
way to surface it.

**Alternatives considered:**

- Report every file whose declarations are all candidates. This would flag
  imported barrels and files imported only by dead code before the cascade,
  producing findings that overlap the symbol report and are not independent
  patches.
- Keep only symbol findings. Knip 5.88.1 and fallow 3.45.0 report unimported
  modules as unused files, and an unimported side-effect module has no
  candidate at all.

**Why:** Both competitors' file rules are import-level. On frozen pre-change
snapshots, the 8 files Knip and fallow both report in requo and the 8 in
accounteer are exactly the ones this rule reports; no competitor-agreed file is
missing or extra. The module-liveness check is what keeps the rule consistent
with build evidence and unknown-scope widening, which can make a module live
without adding a concrete incoming edge.

**Consequences:**

- `GraphSnapshot.unusedFiles` is part of the snapshot, the graph schema, and the
  CLI scan output; an unused file makes `shrike scan` exit with findings.
- Requo reports 9 files and 669 candidates: the 8 competitor-agreed files plus
  `lib/config/index.ts`, which is new and unimported in the working tree and
  postdates both competitor scans. Accounteer reports 8 files / 161 candidates,
  next-portfolio 3 files / 8 candidates, markalvarez 3 files / 24 candidates,
  and portfolio 0 files / 1 candidate.
- `unusedFiles` is recomputed after build-evidence liveness so a build-reached
  file leaves the list.
- The `unused-files` corpus case pins an unimported module, an unimported
  side-effect module, a dead file imported by another dead file, and an
  imported barrel that must stay out of the list.

**Revisit if:** a real project loads an unimported module through a runtime
contract that is not modeled as an entry (for example a convention the
framework reads only by filename), or a competitor starts treating imported
barrels as unused files.

---

## Open-decision resolutions (2026-11-20)

The questions in `MASTER_PROMPT.md` §25 are resolved here with the reasoning
behind the implementation. A blocked item is not a code decision; it needs an
external app, credential, or participant.

| ID | Resolution | Why |
|---|---|---|
| Q-001 | TypeScript host owns the CLI; Rust owns analysis | D-002: config execution and process orchestration belong in Node |
| Q-002 | Worktree by default; in-place explicit | Protects the reviewer's tree and makes verification reproducible from a commit |
| Q-003 | Package entry/exports, `.d.ts`, and `publicApi` globs are public | Conservative: a public surface is never a deletion candidate |
| Q-004 | fast = typecheck+lint; standard adds test; full adds build | Matches CI cost tiers; standard is the default |
| Q-005 | Content-address first; signing deferred | A hash is verifiable without key distribution; signing is additive |
| Q-006 | Read-only trial kit; no network; `.shrike/` only | A partner can evaluate without granting write trust |
| Q-007 | Registry availability blocked; local tarball gate green | Package-name and publish checks need credentials |
| Q-008 | Next.js 16.4+ full; 16.1–16.3 partial; older unsupported | The export contract is only complete from 16.4 |
| Q-009 | Protocol/snapshot `1.0` checked; mismatch fails closed | A cached proof from an unknown version is not a proof |
| Q-010 | Workspaces from `package.json` and `pnpm-workspace.yaml` | Native monorepo semantics without a new config surface |
| Q-011 | One engine process per command | Bounded failure, timeout, and no daemon lifecycle |
| Q-012 | Test/storybook entries included under local policy | Local cleanup should not report test-only code as dead |
| Q-013 | Configured globs plus package/declaration surfaces | No unstated public-API heuristics |
| Q-014 | No per-version schema; tolerate additive fields | The export is young and versioned; strict parsing would block valid data |
| Q-015 | Normalize virtual/module IDs; unmatched stays unknown | An unjoined virtual module must not confirm deletion |
| Q-016 | Consume by default; `--run-build` opt-in | Never make a build a hidden dependency of analysis |
| Q-017 | Claude `Stop`, Cursor `afterFileEdit`+`stop`, OpenCode `session.idle` | Each host's documented task-end event |
| Q-018 | Workflow `GITHUB_TOKEN` via `git`/`gh`; no App | Least privilege; prune refuses forks |
| Q-019 | Task-end hook, advisory and non-blocking | A hook must not hang or mutate the tree |
| Q-020 | Labeled set blocked at 10–20 repositories | Precision without a sample is a claim, not a measurement |
| Q-021 | Per-repository results deferred | Publication needs the labeled set and partner consent |
| Q-022 | Trial reports stay local; aggregates need permission | No source upload and no surprise publication |
| Q-023 | Native first; WASM fallback | Native meets the memory and startup budgets |
| Q-024 | Linux/macOS x64+arm64, Windows x64 planned | D-011 distribution matrix; publishing is blocked |
| Q-025 | Only present-in-both, newly unreachable symbols are orphans | A rename or deletion is not a cleanup target, so it cannot become a false orphan |
| Q-026 | Full checks first; affected-only deferred | Measure real check cost before narrowing coverage |
| Q-027 | Client-reference manifest optional | Its absence degrades to static reachability, never a blocked run |
| Q-028 | Free CLI and design-partner trials in v1 | No hosted product or pricing before the wedge is proven |
| Q-029 | `batchPolicy` caps: 50 symbols, 10 files, 5% live graph | D-013 bounded blast radius, configurable with an explicit override |
| Q-030 | JSON receipt plus Markdown PR body/comment | Machine proof and human review from one artifact |

## Decision index

| ID | Decision | Status |
|---|---|---|
| D-001 | Greenfield application | accepted |
| D-002 | Rust engine, TypeScript host | accepted |
| D-003 | Whole-graph PR orphan delta | accepted |
| D-004 | Verified deletion batches with bisection | accepted |
| D-005 | Portable receipt | accepted |
| D-006 | Build graph optional | accepted |
| D-007 | Next.js/React depth | accepted |
| D-008 | No slop score | accepted |
| D-009 | PR first, never auto-commit | accepted |
| D-010 | Fail closed on unknown edges | accepted |
| D-011 | npm CLI + platform binaries | accepted |
| D-012 | Deterministic, content-addressed outputs | accepted |
| D-013 | Blast-radius guard | accepted |
| D-014 | Hook/MCP are distribution | accepted |
| D-015 | TS/JS only | accepted |
| D-016 | No dashboard in v1 | accepted |
| D-017 | SCC-first cascade, deterministic batches | accepted |
| D-018 | Fail closed on protocol errors | accepted |
| D-019 | Server actions are always entries | superseded by D-020 |
| D-020 | Server actions follow the graph by default | accepted |
| D-021 | Test and script modules are entries; their exports follow the graph | accepted |

## Rejected ideas

| Idea | Why rejected |
|---|---|
| A 0–100 cleanup score | Uncalibrated, gameable, wrong metric |
| Auto-commit to `main` | Incident risk; destroys trust |
| A plugin marketplace | Breadth race; false-positive surface |
| Multi-language v1 | Verification is framework-specific |
| Hosted analysis service | Source upload; new trust boundary; not needed to prove the wedge |
| TypeScript-only engine | Memory and determinism budgets |
| Pure Rust CLI | Config execution and verifier need Node |
| Bundle optimization advice | Vercel owns the category |
| Runtime tracing in v1 | Different product, different trust model |
| Changed-file-only audit | Hides the most valuable orphans |
