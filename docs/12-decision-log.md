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

## D-023 — Cypress specs and support files are module entries

**Decision:** When the owning package depends on `cypress`, the literal values
in `cypress.config.*` name the files Cypress executes: e2e specs from
`e2e.specPattern` (default `cypress/e2e/**/*.cy.{js,jsx,ts,tsx}`) minus
`e2e.excludeSpecPattern`, and the support module from `e2e.supportFile`
(default `cypress/support/e2e.{js,jsx,ts,tsx}`). Component patterns apply only
when the config declares a `component` object. Each matched spec and support
file is a module entry, so its imports stay live while its unimported named
exports remain candidates. Computed config values fall back to Cypress's
documented defaults rather than guessing at executable code.

**Alternatives considered:**

- Protect everything under `cypress/`. Knip 5.88.1 and fallow 3.45.0 both
  report unused exports in Cypress helpers and unimported files in the same
  tree, so a directory-wide entry would hide real findings.
- Seed every export of a spec or support file. Cypress executes the module and
  calls its hooks; it does not consume arbitrary named exports, matching D-021's
  test-runner rule.
- Trust computed patterns or execute the config. The config is executable
  JavaScript; evaluating it is a security and determinism boundary, and a wrong
  guess can protect genuinely dead files. Literal strings cover the documented
  configuration surface and fall back safely.
- Apply the component default unconditionally. `**/*.cy.*` is broad enough to
  match unrelated files in an e2e-only project, so component patterns require an
  explicit `component` object.
- Gate only on the config file existing. A stray or copied `cypress.config.*` in
  a package that does not use Cypress would hide dead files; the dependency gate
  keeps the convention scoped to actual Cypress projects.

**Why:** On `/tmp/opencode/cypress-probe`, Shrike before the change reported 6
candidates (`components/widget.tsx#unusedWidgetHelper`,
`lib/command-helper.ts#commandHelper`,
`lib/command-helper.ts#unusedCommandHelper`, `lib/dead.ts#dead`,
`lib/spec-helper.ts#specHelper`, `lib/spec-helper.ts#unusedSpecHelper`) and 4
unused files (`cypress/e2e/admin.cy.ts`, `cypress/e2e/home.cy.ts`,
`cypress/support/e2e.ts`, `lib/dead.ts`). Knip 5.88.1 and fallow 3.45.0 both
report exactly 1 unused file (`lib/dead.ts`) and 3 unused exports
(`unusedWidgetHelper`, `unusedCommandHelper`, `unusedSpecHelper`). After the
change Shrike reports the same 4 candidates (the 3 unused exports plus
`lib/dead.ts#dead`) and the same single unused file.

**Consequences:**

- Specs and support modules carry framework `cypress` and kinds `spec` /
  `support`; a custom pattern that also matches the generic `.spec.` convention
  is entered once, as a Cypress spec.
- The config reader is package-gated and literal-only; a missing or unreadable
  manifest leaves the convention off.
- The two-sided `cypress-tests` corpus case pins the discovered specs, the
  default support module, and their imported helpers as live, while unused
  helper exports and an unrelated dead file remain reportable.
- `cypress.config.*` remains an ordinary config file rather than a module entry.

**Revisit if:** a Cypress project loads specs through a dynamic pattern that no
literal config value describes, or a competitor starts protecting the whole
`cypress/` tree.

---

## D-024 — `scripts/` files are entries only when referenced

**Decision:** A source file under a root `scripts/` directory is not an entry
because of its location, a documented `Usage:`/`Run:` header, or a shebang. It
enters the graph like any other module: a `package.json` `scripts` or `bin`
entry names it, a live module imports it, or an explicit configuration
references it. Once entered, its exports are helpers and follow the graph
(D-021).

**Alternatives considered:**

- Keep the blanket `scripts/` entry rule. Knip 5.88.1 and fallow 3.45.0 both
  report uninvoked one-off tools; the rule hid seven requo scripts and two
  accounteer scripts that both competitors report.
- Keep only the documented-header heuristic. Requo's five TypeScript tools use
  `Run:` headers and accounteer's two use `Usage:`; both competitors report all
  seven. A header is prose, not a reference.
- Keep the shebang heuristic. Requo's two `.mjs` walkthrough tools have
  shebangs and are still unreferenced; both competitors report them. A shebang
  says how to run a file, not that anything runs it.
- Report every declaration under `scripts/` as a candidate without making the
  file an unused file. That would flag helpers in package-script tools whose
  top-level code calls them; the graph already keeps those live.

**Why:** On requo, Shrike protected `scripts/backfill-pack-assignments.ts`,
`scripts/backfill-pack-recipes.ts`, `scripts/bootstrap-admin.ts`,
`scripts/generate-config-defaults.ts`, `scripts/mark-migrations-applied.ts`,
`scripts/marketing/extract-windows-cursors.mjs`, and
`scripts/marketing/record-landing-walkthrough.mjs`, all reported by both
competitors. On accounteer, Shrike protected `scripts/evaluate-fit.ts` and
`scripts/generate-taxonomy-migration.ts`, also reported by both. After the
change Shrike reports all nine as unused files with their declarations as
candidates, while package-script tools and the helpers they call stay live.

**Consequences:**

- The `executable` summary field and the `looks_executable` heuristic are
  removed. A shebang or a leading documentation header no longer affects graph
  liveness.
- The two-sided `unreferenced-scripts` corpus case pins a package-named script
  as a live entry with one reportable unused export, and pins an unreferenced
  `Run:` tool and an unreferenced shebang tool as unused files with reportable
  declarations.
- `examples/`, `example/`, `demo/`, `playground/`, `playgrounds/`, and
  `benchmarks/` keep their directory entry rule; they are demonstration roots
  rather than ad-hoc tools.

**Revisit if:** a real repository invokes scripts through a path that cannot be
statically named (for example a command table) and both competitors protect it.

---

## D-025 — `"use client"` server-only exports are advisory warnings

**Decision:** A `"use client"` module that exports a Next.js server-only,
route-config, or route-handler name gets an advisory `invalid_client_export`
warning. The name set is fallow 3.45.0's `ILLEGAL_CLIENT_EXPORTS` (22 names,
including the seven HTTP method handlers). The rule is gated on the project
declaring `next` in the root or any workspace manifest. It never changes
candidates, entries, or unused files.

**Alternatives considered:**

- Treat the export as dead. Next.js fails the build, so the code is not
  cleanly deletable until the developer moves it; a dead-code claim would be
  wrong.
- Gate on the owning package declaring `next`. A shared `"use client"` module
  in a React-only workspace package is still compiled in the consuming Next
  app's context; fallow flags it and so does Shrike.
- Include re-exports. Fallow leaves `export { metadata } from "./server"`
  alone, and the re-exported binding belongs to another module.
- Emit one warning per declaration and per local export. Fallow reports one
  finding per exported name; Shrike anchors a direct export on the declaration
  and a local export on its specifier.

**Why:** Reverse-engineered from fallow's source
(`crates/core/src/analyze/invalid_client_exports.rs`) and confirmed with probes
under `/tmp/opencode/client-export-*`: Shrike and fallow agree on all 22 names,
the same file/line anchors, the monorepo gate, and a no-Next control. Knip
5.88.1 has no equivalent rule. `declaration.directly_exported` is needed
because `mark_local_exports` sets `exported` for a local alias; without it
`const metadata; export { metadata as config }` would be a false positive.

**Consequences:**

- `DeclarationRecord.directly_exported` is added with `serde(default)` and set
  by every declaration constructor; `mark_local_exports` leaves it unchanged.
- `ANALYSIS_VERSION` is bumped to 61 so cached summaries rebuild.
- The warning groups under `boundary` and is actionable in the CLI diagnostic
  summary.
- Corpus candidates are unchanged; the rule is diagnostics-only.

**Revisit if:** Next.js starts allowing a server-only name through a client
boundary under a documented condition, or fallow's list diverges.

---

## D-026 — App Router route conflicts are error diagnostics

**Decision:** Shrike reports two Next.js App Router conflicts as error
diagnostics, never as dead-code findings: `route_collision` when two
`page`/`route` leaves in one app root resolve to the same URL, and
`dynamic_segment_name_conflict` when sibling dynamic segments at one tree
position use different spellings. Both reuse one path-based route-tree
primitive. The app root is `<package>/app` or `<package>/src/app`, anchored on
the owning package; route groups are transparent, parallel slots fork the URL
namespace, and private/intercepting subtrees are excluded. The checks require
the owning package to declare `next`.

**Alternatives considered:**

- Treat a collision as a dead-code finding. Neither file is necessarily dead:
  the developer may delete either owner, or move one to a different URL. A
  deletion claim would be wrong.
- Use fallow's project-wide Next gate. That flags a React-only workspace
  package with an app-shaped directory whenever a sibling declares `next`,
  even though that package's directory is not a Next app root. Shrike's
  per-package gate is the same precision it already uses for Next entry
  conventions.
- Only check `page` files. A `route` handler and a page share one URL
  namespace in Next.js, so page+route is a collision too.
- Treat different dynamic spellings as a route collision. `next build`
  succeeds on that shape; the failure is at request time, so it gets a
  separate diagnostic.

**Why:** Read fallow 3.45.0's `route_tree.rs`, `route_collision.rs`, and
`dynamic_segment_name_conflict.rs`. On `/tmp/opencode/route-conflict-probe`,
fallow reports 8 collisions and 3 dynamic conflicts; Shrike reports the same
8 + 3 once the sibling package declares Next, and only 6 + 3 when it does not.
The probe covers route groups, page+route, the root URL, parallel slots,
`[id]` vs `[slug]`, a layout proving a dynamic directory, private folders,
intercepting routes, and a React-only package control.

**Consequences:**

- The new diagnostics group under `routing` and are actionable in the CLI
  summary. They do not change candidates, entries, or unused files.
- `ANALYSIS_VERSION` is bumped to 62 so cached snapshots include the new
  diagnostics.
- Page-extension parsing is extracted to
  `Builder::configured_page_extensions` so entry detection and route-conflict
  detection share the same dotted-extension handling.

**Revisit if:** Next.js allows two URL owners under a documented condition, or
fallow's route-tree semantics diverge.

---

## D-027 — Misplaced RSC directives are advisory warnings

**Decision:** A `"use client"` or `"use server"` string that appears after the
leading prologue produces a `misplaced_directive` warning. RSC bundlers
silently ignore it, so the intended client or server boundary is never
created. The rule is gated project-wide on an RSC bundler dependency: `next`,
`waku`, `@lazarv/react-server`, `react-server-dom-webpack`,
`react-server-dom-vite`, `react-server-dom-parcel`, or `@vitejs/plugin-rsc`
in the root or any workspace manifest. It never changes candidates, entries,
or unused files.

**Alternatives considered:**

- Treat the module as a client boundary anyway. The bundler does not, so the
  analysis would be wrong about the file's semantics.
- Flag only in Next.js. Waku and the `react-server-dom-*` toolchains honor the
  same directives, and fallow gates on the same seven packages.
- Gate on the owning package. A shared module can be compiled by a sibling
  RSC app; the directive is ignored there regardless of the module's own
  manifest.
- Flag `"use strict"`. It is a harmless JavaScript prologue directive with no
  RSC meaning.

**Why:** Read fallow 3.45.0's `misplaced_directive.rs`,
`predicates/import.rs`, and the extract visitor. On
`/tmp/opencode/misplaced-directive-probe`, fallow reports three findings
(`app/late-client.tsx#2`, `app/late-server.ts#2`,
`packages/plain/src/late-client.ts#2`) and Shrike reports the same three at
the same lines. Both exclude a leading directive, a nested string, and
`"use strict"`. The React-only control reports zero in both tools, and a Waku
probe reports the same single finding in both.

**Consequences:**

- The parser records `FileSummary.misplaced_directives` with a span; the
  leading directives remain in `FileSummary.directives` only.
- `ANALYSIS_VERSION` is bumped to 63 so cached summaries include the new
  record.
- The warning groups under `boundary` and is actionable in the CLI diagnostic
  summary.
- Corpus candidates are unchanged; the rule is diagnostics-only.

**Revisit if:** an RSC bundler starts honoring body-position directives, or
fallow's bundler predicate diverges.

---

## D-028 — Mixed client/server barrels are advisory warnings

**Decision:** A barrel that directly re-exports a `"use client"` origin and a
server-only origin produces a `mixed_client_server_barrel` warning anchored on
the earlier re-export. A server-only origin carries `"use server"`, imports
`server-only`, `next/server`, `fs`/`fs/promises`/`child_process` (bare or
`node:`), or imports a named `cookies`/`headers`/`draftMode` API (or the whole
namespace) from `next/headers`. Type-only re-exports and type-only imports are
ignored, and only direct local origins are classified. The rule is project-wide
and gated on the seven RSC bundlers in D-027. It never changes candidates,
entries, or unused files.

**Alternatives considered:**

- Match fallow exactly, including type-only imports. `import type { cookies }
  from "next/headers"` is erased and does not put server code in the client
  bundle, so flagging it would be a false positive.
- Treat any re-export from a server module as mixed. A client component plus an
  ordinary utility is a normal barrel and must not flag.
- Walk transitive re-export chains. Direct origins keep the rule precise and
  avoid npm conditional-export ambiguity.
- Report the barrel as a dead-code finding. The barrel can be perfectly live;
  the problem is a boundary shape, not liveness.

**Why:** Read fallow 3.45.0's `mixed_barrel.rs`, `server_only.rs`, and
`predicates/import.rs`. On `/tmp/opencode/mixed-barrel-probe`, fallow reports
four barrels and Shrike reports the same three real ones at the same lines and
origins; the fourth is `src/barrel-type-only-server.ts`, whose only server-side
signal is `import type { cookies } from "next/headers"`. A plain utility
barrel and a type-only client re-export produce zero in both tools. A
React-only control produces zero in both, and a Waku probe fires in both.

**Consequences:**

- `ANALYSIS_VERSION` is bumped to 64 so cached snapshots include the new
  diagnostics.
- The warning groups under `boundary` and is actionable in the CLI diagnostic
  summary.
- Corpus candidates are unchanged; the rule is diagnostics-only.

**Revisit if:** React Server Components tooling starts honoring type-only
imports at runtime, or fallow removes its type-only false positive.

---

## D-029 — Client/server leak cones are advisory warnings

**Decision:** A `"use client"` module whose runtime import cone reaches a
server-only sink produces a `client_server_leak` warning. The cone follows
runtime static imports, re-exports, export-alls, and non-worker dynamic
imports; type-only edges are erased. The sink predicate is D-028's:
`server-only`, `next/server`, `fs`/`fs/promises`/`child_process` (bare or
`node:`), or a named `cookies`/`headers`/`draftMode` API (or namespace) from
`next/headers`. An all-action `"use server"` module stops the cone; a module
with any non-action value export keeps walking. A
`next/dynamic(..., { ssr: false })` edge is skipped when the same specifier has
no runtime static edge. The rule is project-wide over the seven RSC bundlers in
D-027, emits one warning per client root anchored on the first import of the
shortest path, and never changes candidates, entries, or unused files.

**Alternatives considered:**

- Detect leaks only from build evidence. `client-boundary-leak` already does
  that, but it requires a successful Next build and only sees modules the
  bundler emitted. The static cone reports the same failure before the build.
- Follow type-only edges. They are erased before bundling and cannot put
  server code in the client bundle.
- Stop at every `"use server"` module. A module that also exports a top-level
  constant or default value can ship its imports to the client, so the cone
  must keep walking.
- Treat every `next/dynamic` import as client-only. Without `{ ssr: false }`
  the component still renders on the server, so its imports are ordinary
  client edges.
- Treat a bare `import "next/headers"` as a sink. It has no runtime binding and
  cannot call `cookies`/`headers`/`draftMode`; a namespace or named import can.
- Report one warning per sink. One warning per client root keeps the output
  bounded and names the shortest path to the first sink, matching fallow.

**Why:** Read fallow 3.45.0's
`crates/core/src/analyze/security/mod.rs` (BFS, action-module and
`ssr: false` exclusions) and `crates/core/src/analyze/server_only.rs`. On
`/tmp/opencode/client-leak-probe`, fallow reports four server-only findings
(`app/action-mixed/page.tsx`, `app/direct/page.tsx`,
`app/dynamic-no-ssr/page.tsx`, `app/transitive/page.tsx`) and Shrike reports
the same four at the same lines after dynamic imports gained real spans. Both
exclude the all-action module, the type-only edge, the `ssr: false` escape
hatch, the static-edge-plus-`ssr:false` case, and the server component. A
React-only control reports zero in both, and a Waku probe fires in both.

**Consequences:**

- `FileSummary.client_only_dynamic_specifiers` records the specifiers of
  `next/dynamic(..., { ssr: false })` calls, and dynamic-import records now
  carry source spans. `ANALYSIS_VERSION` is bumped to 65.
- The warning groups under `boundary` and is actionable in the CLI diagnostic
  summary.
- Corpus candidates are unchanged; the rule is diagnostics-only.

**Revisit if:** Next.js changes when `next/dynamic` with `ssr: false` is
client-only, or fallow's leak-cone exclusions diverge.

---

## D-030 — Async arrow actions and inline directives are Server Actions

**Decision:** The all-action `"use server"` predicate mirrors fallow 3.45.0's
`is_server_action_module`: a leading directive plus every value export
async-function-shaped. Async function declarations, async arrow functions, and
async function expressions count; type exports, ambient declarations, and
overload signatures are allowed because they have no runtime value; a value
re-export or any other value export keeps the module out of the shape. A local
`export { local }` or `export default local` resolves from its top-level
declaration, with only `const` async function expressions classified as
actions.

An inline `"use server"` body directive is recorded for exported function
declarations, exported variable declarators, and default-exported functions or
arrows. Under `serverActionPolicy: "protect"`, with RSC bundler evidence and no
file-level directive, each inline action export is seeded as an entry.
Whole-file `"use server"` protection is unchanged: every export of a module with
the file-level directive is seeded under `protect`.

**Alternatives considered:**

- Only async function declarations (the previous Shrike behavior): misses the
  common `export const action = async () => {}` form, so the leak cone falsely
  reports a sanctioned boundary and `protect` leaves a real action dead.
- Treat any `"use server"` file as all-action regardless of other exports: a
  top-level constant or value re-export can carry the module's imports into the
  client bundle, hiding a real leak.
- Treat an inline directive as a file-level directive: it would stop the leak
  cone at a module whose other imports still ship, and it would protect
  unrelated exports.
- Resolve value re-exports across modules: the predicate cannot classify the
  target from this module; the conservative `false` keeps the cone walking.
- Protect inline actions without RSC bundler evidence: outside an RSC bundler
  the directive has no meaning.
- Capture only named inline actions, as fallow's exported-only capture does: a
  default-exported inline action is legal Next.js and would otherwise be
  reported dead. Shrike records `"default"` as well.

**Why:** Read fallow 3.45.0's
`crates/extract/src/visitor/visit_impl_server_actions.rs` (module shape),
`declarations.rs` (inline capture), and
`crates/core/src/analyze/unused_server_action.rs` (reclassification). On
`/tmp/opencode/server-action-probe`, fallow's `security` reports six
client-server leaks (`class`, `default-sync`, `export-star`, `mixed`,
`reexport`, `sync`) and Shrike reports the same six at line 3. Both exclude the
async function, async arrow, async function expression, type-plus-action,
local-alias, default-local, anonymous-default, `let` async arrow, and type-only
re-export modules. Fallow's `dead-code` classifies the two unreferenced inline
actions as `unused-server-action` and the ordinary export as `unused-export`;
Shrike's default `graph` policy reports all three as candidates, and `protect`
keeps the inline actions live. Next.js's own `server-boundary.js` validation
requires `"use server"` exports to be async functions.

**Consequences:**

- `FileSummary.server_action_module` and
  `FileSummary.inline_server_action_exports` record the two predicates;
  `ANALYSIS_VERSION` is bumped to 66.
- Graph tests cover an all-arrow module stopping the leak cone, a mixed arrow
  module still leaking, inline actions under `protect` and `graph`, and the
  missing-RSC-bundler control.
- The inline capture is exported-only and declaration-attached, matching
  fallow. A local `export { action }` alias of an inline action declared
  elsewhere is not inferred, and the conservative choice keeps the leak cone
  walking.

**Revisit if:** React or Next.js starts allowing non-async `"use server"`
exports, or fallow changes its all-action predicate or inline capture.

---

## D-031 — Definitely non-function `"use server"` exports are warnings

**Decision:** A leading `"use server"` module's value exports that are
definitely not callable produce an advisory `invalid_server_export` warning.
The parser records the exported name and a span for literals, objects, arrays,
class expressions, `new`, tagged templates, `await`, missing initializers, and
non-ambient enums. It unwraps parentheses and TypeScript `as` / `satisfies` /
`!` / type-assertion / instantiation wrappers first. Arrows, function
expressions, calls, and identifiers are skipped because they may be
Promise-returning functions. Direct class declarations, default
classes/functions/interfaces, type-only exports, and value re-exports are
skipped because Next's runtime validator sees a class as a function and its
TypeScript plugin only checks Promise-returning types. A local
`const value; export { value }` is resolved from its top-level declaration and
anchored on the specifier; direct declarations are anchored on the declarator
or enum. The rule is gated on the project declaring an RSC bundler.

**Alternatives considered:**

- Report every non-async export: Next.js requires async functions, but a call
  or identifier may return a function or a Promise; the definite subset is the
  only build-failure claim Shrike can support without type information.
- Report direct class declarations: Next's flight loader checks
  `typeof action === 'function'`, which is true for a class; its TS plugin does
  not handle a direct class export, so a warning would be a false positive.
- Report value re-exports: the target module owns the value; classifying it
  here would require cross-module analysis and the runtime failure belongs to
  the target's export.
- Report enums only when non-const: a const enum is also an object at runtime
  in non-isolatedModules builds; both are definitely not functions. Ambient
  `declare enum` is erased and is skipped.
- Treat an inline body directive as file-level: the module's other exports are
  ordinary values, so a module-level rule would misreport them.

**Why:** Read Next.js's flight loader
(`dist/build/webpack/loaders/next-flight-loader/action-validate.js`), which
throws when an action export is not a function, and its TypeScript plugin
(`dist/build/webpack/plugins/next-types-plugin/server-boundary.js`), which
requires Promise-returning types for `"use server"` exports. Its
variable-statement check rejects any initializer that is not an arrow,
function expression, call, or identifier, which covers class expressions and
`await` even though both pass the runtime `typeof` check. Fallow 3.45.0 does
not report this condition; knip 5.88.1 does not either. A probe with a Next.js
app shows the runtime validator rejecting literal, object, array, enum,
local-alias, `new`, and tagged-template exports while accepting async arrows,
function expressions, calls, identifiers, classes, class expressions, and
re-exports.

**Consequences:**

- `FileSummary.invalid_server_exports` records the findings;
  `ANALYSIS_VERSION` is bumped to 67.
- Parser and graph tests cover the definite forms, the conservative skips,
  anchors, project gating, severity, and the message.
- The warning groups under `boundary` in the CLI diagnostic summary and never
  changes candidates or entries.

**Revisit if:** Next.js changes its action validator or TypeScript plugin, or
if Shrike gains type information that can prove a call or identifier is not a
function.

---

## D-032 — Invalid route segment configuration is an error diagnostic

**Decision:** App Router files that combine route-segment configuration
Next.js rejects produce an `invalid_route_segment_config` error diagnostic.
The parser records only the export shapes Next.js's `checkExports` and
`extractExportedConstValue` recognize, and the graph checks the six definite
combinations: both parameter-matching exports; parameter matching in a
`route` handler, with `runtime = 'edge'`, or with `"use client"`;
`generateStaticParams` with `runtime = 'edge'` or `"use client"`;
`instant` / `prefetch` / `ensureStatic` in a `"use client"` module; and
`unstable_dynamicStaleTime` in a layout or next to `instant`.

**Alternatives considered:**

- Match export names without the parser shapes: a function named `instant`, a
  re-exported `runtime`, or a parenthesized initializer is not read by Next.js
  and would produce false positives.
- Treat every extractable value as valid config: Next.js's schema rejects
  invalid values with a different error, so the rule checks only values that
  satisfy the relevant schema.
- Gate project-wide: a React-only workspace package with an app-shaped
  directory is not a Next app; the per-package gate matches the route-conflict
  checks.
- Report warnings: Next.js throws before the build completes, so these are
  errors, not advisories.

**Why:** Read Next.js 16.4.6
`dist/build/analysis/get-page-static-info.js` and
`dist/build/analysis/extract-const-value.js`, and probed
`getAppPageStaticInfo` with each combination. The runtime throws for all six
groups; it does not throw for a function named `instant`, a re-exported
`runtime`, a parenthesized `runtime`, a `let` `runtime`, a segment config in a
non-page module, or an app-shaped directory in a package without `next`.

**Consequences:**

- `FileSummary.next_segment_exports` and `FileSummary.exported_const_values`
  record the parser facts; `ANALYSIS_VERSION` is bumped to 68.
- Parser and graph tests cover the recognized shapes, the conservative skips,
  the six combinations, anchors, severity, and project gating.
- The error groups under `routing` in the CLI diagnostic summary and never
  changes candidates or entries.

**Revisit if:** Next.js changes its segment-config checks, adds new
`cacheComponents`-gated options, or starts validating values Shrike can model.

---

## D-033 — Invalid client exports are route-only build errors

**Decision:** A `"use client"` page, layout, or route handler that exports a
Next.js server-only API produces an `invalid_client_export` error diagnostic.
Pages and layouts reject `metadata`, `generateMetadata`, `viewport`,
`generateViewport`, and `revalidate`; route handlers reject `metadata`,
`generateMetadata`, and `revalidate`. The rule is gated per package on `next`
and applies only to files classified as `page`, `layout`, or `route`.

**Alternatives considered:**

- Keep fallow 3.45.0's 22-name list: real Next.js 16.4.6 builds succeed for
  `dynamic`, `dynamicParams`, `fetchCache`, `runtime`, `preferredRegion`,
  `maxDuration`, the HTTP method handlers, and the legacy Pages Router data
  functions, so reporting them would be a false positive.
- Keep the project-wide gate: a `"use client"` module in a non-route file does
  not fail a Next.js build, and a route in a package that does not declare
  `next` is not a Next app.
- Report warnings: Next.js fails the build, so these are errors.
- Exclude re-exports and type-only specifiers: Next.js flags
  `export { metadata } from "./server"`, `export type { metadata } from`, and
  `export * as metadata`; only local `interface` and `type` declarations are
  erased and ignored.

**Why:** Probed Next.js 16.4.6 with real `next build` runs. Client page and
layout builds fail for the five names; client route handlers fail for
`metadata`, `generateMetadata`, and `revalidate` but succeed for `viewport` and
`generateViewport`; `template`, `not-found`, `error`, `loading`, and non-route
client modules succeed; `dynamic`, `dynamicParams`, `fetchCache`, `runtime`,
`preferredRegion`, `maxDuration`, and `GET` succeed. Server controls confirm
that `export type { metadata }` and local aliases are client-specific, while
local `interface` and `type` declarations are ignored.

**Consequences:**

- `NEXT_SERVER_ONLY_EXPORTS` is narrowed to the five verified names and
  `NEXT_ROUTE_HANDLER_SERVER_ONLY_EXPORTS` to the three route-handler names.
- The detector uses `classify_app_route` and the per-package Next gate.
- `ANALYSIS_VERSION` is bumped to 69.
- The error groups under `boundary` in the CLI diagnostic summary and never
  changes candidates or entries.

**Revisit if:** Next.js changes its client-boundary export validation or starts
failing for the names that currently build.

---

## D-034 — Pages Router build failures are error diagnostics

**Decision:** A Pages Router page whose data-fetching exports or runtime
configuration Next.js rejects produces an `invalid_pages_router_export` error
diagnostic. The definite combinations are `getStaticProps` with
`getServerSideProps`; `getInitialProps` on the default component with either;
`getStaticPaths` with `getServerSideProps`; `getStaticPaths` without
`getStaticProps` when there is no `getInitialProps` or `getServerSideProps`;
`getStaticPaths` on a non-dynamic page with `getStaticProps`; a dynamic page
with `getStaticProps` but no `getStaticPaths`; `runtime: 'edge'` on a non-API
page; and a runtime value outside `nodejs`, `edge`, and
`experimental-edge`. The rule is package-relative under `pages/` or
`src/pages/`, honors a configured `pageExtensions`, and is gated on the owning
package declaring `next`.

**Alternatives considered:**

- Report `getInitialProps` with `getStaticPaths`: real Next.js 16.4.6 builds
  succeed; only runtime behavior differs, so this would be a false positive.
- Report every `getStaticPaths` without `getStaticProps`: builds succeed when
  `getInitialProps` or `getServerSideProps` supplies the data.
- Match export names without the parser shapes: type-only exports, a
  default-exported component named `getStaticProps`, and
  `export * as getStaticProps` are not read by Next.js and would produce false
  positives.
- Treat any `Page.getInitialProps` assignment as present: Next.js checks
  truthiness, so `Page.getInitialProps = undefined`, `null`, `false`, `0`,
  `""`, and `void` do not count.
- Apply the data-function checks to `pages/api`, `_app`, `_document`, and
  `_error`: Next.js skips them there. Runtime config is still validated, but
  API routes accept `edge`.
- Gate project-wide: a `pages` directory in a React-only workspace package is
  not a Next app.
- Report warnings: Next.js throws before the build completes, so these are
  errors.

**Why:** Probed Next.js 16.4.6 with real `next build` runs at
`/tmp/opencode/next-pages-probe`. Confirmed failures: `getStaticProps` with
`getServerSideProps`; `getInitialProps` with either; `getStaticPaths` with
`getServerSideProps`; non-dynamic `getStaticProps` with `getStaticPaths`;
dynamic `getStaticProps` without `getStaticPaths`; `getStaticPaths` without
`getStaticProps`; `runtime: 'edge'` on a non-API page (top-level or
`config.runtime`) and on `_app`; and invalid or non-string runtime values,
including on API routes. Confirmed successes: `getInitialProps` with
`getStaticPaths` on dynamic and non-dynamic pages, `runtime:
'experimental-edge'`, `runtime: 'edge'` on an API route, and
`config = { revalidate: 'nope' }`. The failure comes from the loaded page
module check in `dist/build/utils.js` and the Pages Router segment schema in
`dist/build/segment-config/pages/pages-segment-config.js`; fallow 3.45.0 and
knip 5.88.1 do not detect these conflicts.

**Consequences:**

- `FileSummary.default_export_local`,
  `FileSummary.default_export_has_get_initial_props`, and
  `FileSummary.pages_config_runtime` record the parser facts, backed by the new
  `PagesRouterInfo`; `ANALYSIS_VERSION` is bumped to 70.
- Parser and graph tests cover the recognized and ignored export shapes, the
  statically falsy assignments, the conflict matrix, the page-extension and
  package gates, the API/reserved exceptions, anchors, severity, and unchanged
  candidates.
- The error groups under `routing` in the CLI diagnostic summary and never
  changes candidates or entries.

**Revisit if:** Next.js changes its Pages Router data-function validation, the
runtime schema, or the reserved-page set.

---

## D-035 — Root-only Next.js conventions require the app root

**Decision:** Next.js loads `robots`, `manifest`, `global-error`, and
`global-not-found` only when the file sits directly under `<package>/app/` or
`<package>/src/app/`. A route group, parallel slot, or nested segment is not a
framework file. `middleware`, `proxy`, `instrumentation`, and
`instrumentation-client` are conventions only at the package root or `src/`,
never under `app/`. `sitemap` and the image-metadata routes remain conventions
at any depth.

**Alternatives considered:**

- Treat every app-shaped stem at any depth as an entry: Next.js ignores
  `app/nested/robots.ts`, `app/(group)/robots.ts`, and
  `app/@slot/robots.ts`, so crediting them hides genuinely dead files.
- Follow knip 5.88.1's `app/{,[(]*[)]/}{manifest,robots}` pattern and allow a
  root-level route group: a real Next.js 16.4.6 build with
  `app/(group)/robots.ts` and `app/(group)/manifest.ts` produces neither
  `/robots.txt` nor `/manifest.webmanifest`.
- Keep `middleware`, `proxy`, and instrumentation as app entries: Next.js
  only recognizes them at the package root or `src/`; an `app/middleware.ts`
  is an ordinary module.
- Remove nested `global-error` from entries only: the app loader resolves it
  but overwrites it with the root global error, so it is not framework-invoked.
  The same probe showed it is not even compiled unless the segment has a page.

**Why:** Probed Next.js 16.4.6 with real `next build` runs at
`/tmp/opencode/next-root-conventions-probe`. A root `app/robots.ts`,
`app/manifest.ts`, and `app/global-error.tsx` are evaluated and routed; the
nested, grouped, and slotted copies are not (their throwing imports never run).
`app/nested/sitemap.ts` is routed as `/nested/sitemap.xml`. Source in
`dist/lib/metadata/is-metadata-route.js` anchors `robots` and `manifest` at the
app root, and `dist/build/webpack/loaders/next-app-loader/index.js` resolves
`global-error` only at the root layer. Knip's group allowance and fallow
3.45.0's any-depth patterns both over-credit.

**Consequences:**

- `NEXT_APP_ANY_DEPTH_STEMS` and `NEXT_APP_ROOT_ONLY_STEMS` drive entry
  detection; `classify_app_route` rejects a nested root-only stem so it cannot
  decorate a dynamic segment or collide with a route.
- Graph tests pin the root, nested, grouped, and slotted forms and the
  dynamic-segment control; `ANALYSIS_VERSION` is bumped to 71.
- The five-repo scans are unchanged: none has a root-only convention outside
  the app root.

**Revisit if:** Next.js changes metadata-route discovery or starts loading
nested `global-error` as a real boundary.

---

## D-036 — Next.js exports are scoped to the file type that reads them

**Decision:** Each Next.js convention file protects only the exports Next.js
reads from that file type, not the union of every framework name. Pages read
`default`, metadata/viewport, static params, parameter matching, and segment
config, plus page-only `unstable_dynamicStaleTime`. Layouts and `default` files
read the same surface without `unstable_dynamicStaleTime`. Route handlers read
HTTP methods, `generateStaticParams`, and segment config, but no
metadata/viewport, `default`, or parameter matching. `not-found`, `forbidden`,
and `unauthorized` read `default` plus metadata/viewport;
`global-not-found`, `template`, `loading`, `error`, and `global-error` read only
`default`. Metadata routes read `default`, `generateStaticParams`, and segment
config, with `generateSitemaps` on sitemap and image metadata on image routes.
Pages Router files follow the same rule: ordinary pages, API routes, `_app`,
`_document`, and `_error` each expose a different surface.

**Alternatives considered:**

- Keep the union on every app or pages entry: it credits `metadata` on a route
  handler, `GET` on a page, `config` on an App Router page, `getStaticProps` on
  an API route, and `reportWebVitals` on a nested `_app`. Next.js never reads
  those names there, so dead exports stay hidden.
- Adopt fallow 3.45.0's per-file sets wholesale: fallow is close, but it omits
  `generateStaticParams` from route handlers, omits parameter matching from
  pages and layouts, gives `global-not-found` the fallback metadata surface, and
  omits `runtime`/`maxDuration` from Pages Router pages, `_app`, and API routes.
  Its metadata-route macro also credits `generateStaticParams` on root
  `robots`/`manifest`, where the generated route has no dynamic segment.
- Adopt knip 5.88.1's entry patterns: knip models entries and the Next.js plugin
  less precisely, so it cannot express the per-stem export surfaces.
- Remove legacy names that Next.js 16 no longer reads, including
  `experimental_ppr`: Shrike documents a 15.x fallback, and Next.js 15.5.4 still
  lists `experimental_ppr` in `AppSegmentConfigSchemaKeys`.

**Why:** Verified against Next.js 16.4.6 source and real builds. `page`,
`layout`, `default`, and `route` surfaces come from
`dist/build/analysis/get-page-static-info.js`, `dist/server/lib/app-dir-module.js`,
and `dist/build/segment-config/app/app-segments.js`. Error-boundary metadata
comes from `dist/lib/metadata/metadata.js`, where an HTTP access `errorType`
selects `not-found`/`forbidden`/`unauthorized`; `global-not-found` renders with
`errorType` undefined. Metadata-route exports come from
`dist/build/webpack/loaders/next-metadata-route-loader.js`, which re-exports
named exports but only reads `default`, `generateSitemaps`, and
`generateImageMetadata`. Pages Router surfaces come from `PagesSegmentConfigSchemaKeys`,
`dist/build/utils.js` (`RESERVED_PAGE`), and `dist/lib/is-api-route.js`.
`unstable_dynamicStaleTime` is page-only in
`dist/server/app-render/app-render.js` and throws in a layout. The
`unstable_` spellings are retained for 16.0-16.2, and `experimental_ppr` for
15.x.

**Consequences:**

- `next_framework_exports` owns the per-stem surface; app and Pages Router
  entries call it instead of sharing `public_names`. `pages/api.{ts,tsx,js,jsx}`
  is the `/api` route, and only root `_app`/`_document`/`_error` are special.
- Graph tests pin each file type's entries and the names that must remain
  candidates, including route-handler metadata, page `config`, `_app`
  `getStaticProps`, nested `_app` `reportWebVitals`, and image `alt` on an icon.
- `ANALYSIS_VERSION` is bumped to 72. The five-repo scans are unchanged because
  none exports a framework-only name from a file type that does not read it.
- `onRequestInit` is not a Next.js 16.4.6 instrumentation export and remains a
  candidate; the docs no longer list it.

**Revisit if:** Next.js unifies the module surfaces, changes metadata-route
re-exports, or removes the 15.x fallback.

---

## D-037 — Route handlers get route-level config, not page/layout cache config

**Decision:** Split the App Router segment-config surface in two. Route-level
config (`dynamic`, `dynamicParams`, `revalidate`, `fetchCache`, `runtime`,
`preferredRegion`, `maxDuration`) is framework API on every segment module:
page, layout, `default`, route handler, and metadata route. Page/layout cache
config (`ensureStatic`, `instant`/`prefetch`, their `unstable_` predecessors,
and legacy `experimental_ppr`) is framework API only on page, layout, and
`default` files. A route handler or metadata route that exports one of those
cache names keeps it analyzable.

**Alternatives considered:**

- Keep the union on every segment module (D-036's initial split): Next.js
  parses the names from every app segment, so this was defensible, but parsing
  is not consumption. A route handler never renders a component tree, so its
  `instant` or `ensureStatic` export can never affect output.
- Make `dynamicParams` page-only too: static-path generation reads it from the
  route segment when a route handler exports `generateStaticParams`, so it is
  route-level config.
- Drop `experimental_ppr` from the page surface because Next.js 16.4.6 no
  longer parses it: Shrike documents a 15.x fallback, and Next.js 15.5.4 still
  lists it in `AppSegmentConfigSchemaKeys`.

**Why:** Verified against Next.js 16.4.6. `dist/build/segment-config/app/app-segments.js`
parses segment config for every module, but `dist/server/route-modules/app-route/module.js`
reads only `dynamic`, `revalidate`, `fetchCache`, and the HTTP methods from a
route handler. `dist/build/static-paths/app.js` reads `dynamicParams`.
`instant`, `prefetch`, and `ensureStatic` are consumed by page and layout
rendering; `dist/server/dev/static-paths-worker.js` gates `ensureStatic` to app
pages. `experimental_ppr` is absent from 16.4.6 and present in 15.5.4's
`AppSegmentConfigSchemaKeys`.

**Consequences:**

- `next_framework_exports` extends `NEXT_APP_ROUTE_CONFIG_EXPORTS` on every
  segment stem and `NEXT_APP_CACHE_CONFIG_EXPORTS` only on `page`, `layout`,
  and `default`.
- Graph tests pin `dynamicParams` as an entry on a route handler and
  `instant`/`prefetch`/`ensureStatic` as candidates on route handlers and
  metadata routes.
- `ANALYSIS_VERSION` is bumped to 73. The five-repo scans are unchanged because
  no route or metadata route in those repos exports a cache-config name.
- Diagnostic rules are untouched: `invalid_route_segment_config` still reads
  cache-config values from any segment module, because Next.js still parses
  them and still rejects the `"use client"` combinations.

**Revisit if:** Next.js starts consuming cache config from route handlers or
metadata routes, or removes the page/layout renderer split.

---

## D-038 — Unused React component props are advisory warnings

**Decision:** Report a destructured prop that a live local React component never
reads as an `unused_component_prop` warning. Never emit it as a deletion
candidate: removing a prop changes the component contract, and Shrike's batch
deletion model only removes whole symbols. Scope the parser to top-level
uppercase function declarations and `const` arrow/function-expression
components that return JSX, with a single inline object-destructured parameter.
Abstain on rest/spread, a bare `props` parameter, multiple parameters, nested
patterns, computed keys, exported components, and any component whose value
escapes the module (`memo(Card)`, a registry, `displayName`, direct export).
Gate the rule project-wide on a React runtime (`react`, `react-dom`, `next`,
`preact`).

**Alternatives considered:**

- Emit a deletion candidate: rejected because a prop is not a symbol and
  removing it is a manual contract change. The warning names the component and
  prop so the reviewer can decide.
- Follow fallow 3.45.0's exported-binding check: fallow reports a prop on
  `export default memo(Card)` because it only asks whether `Card` is exported.
  Shrike's value-reference walk sees `Card` passed to `memo` and abstains, which
  avoids the HOC false positive.
- Report every JSX-returning function: rejected because `forwardRef`, imported
  prop interfaces, and rest-spread components need manual review and would
  widen the false-positive surface.

**Why:** fallow 3.45.0 detects unused props; knip 5.88.1 does not. A probe at
`/tmp/opencode/react-gap-probe` reports `Badge.tone` under both Shrike and
fallow. A second probe at `/tmp/opencode/props-hoc-probe` shows fallow's HOC
false positive (`Card.subtitle`) and Shrike's abstention while still reporting
the local `Local.subtitle`.

**Consequences:**

- `FileSummary.component_props` records the component, prop, local binding,
  usage flag, and span; the parser skips the whole-program value-reference walk
  when a file has no component candidates.
- The graph emits warnings after reachability, so a dead component stays a
  candidate and does not also produce prop warnings.
- `ANALYSIS_VERSION` is bumped to 74.
- The CLI groups the code under the `react` diagnostic category.

**Revisit if:** Shrike adds contract-level edits or a verified prop-removal
workflow.

---

## D-039 — Unused TypeScript enum members are advisory warnings

**Decision:** Report a member of a live, exported TypeScript enum that no
project reference reads as an `unused_enum_member` warning. Never emit it as a
deletion candidate: removing a member changes a serialized value, a numeric
value, or a public contract, and Shrike's batch deletion model only removes
whole symbols. Restrict the rule to top-level, non-ambient enums in
non-declaration source files that are not generated, merged, public API, or
entries. Credit every member on a whole-object use, computed access, numeric
reverse lookup, namespace alias, local or re-export alias, or literal dynamic
import. Credit the referenced member on an initializer (`B = A`) and credit
every member on object destructuring, because the destructured binding cannot
be attributed to one member with confidence.

**Alternatives considered:**

- Emit a deletion candidate: rejected because a member is not a symbol and the
  edit can change a stored value or an external contract. The warning names the
  enum and member for manual review.
- Follow fallow 3.45.0's initializer handling: fallow reports `Initializer.A`
  as unused in `enum Initializer { A = 1, B = A, C = 4 }` even though `B`
  reads `A`. Shrike's parser records initializer member references and credits
  them, which removes that false positive.
- Match knip 5.88.1 on object destructuring: knip reports
  `Destructured.Second` after `const { First } = Destructured`. Shrike credits
  every member because the object is read as a whole; the finding is hidden
  rather than risk a false positive.
- Report members of dead enums: rejected because the enum itself is already one
  deletion candidate; reporting each member would duplicate the finding.

**Why:** fallow 3.45.0 and knip 5.88.1 both detect unused enum members and
Shrike did not. A probe at `/tmp/opencode/enum-probe` has eleven enums covering
string, numeric, whole-object, computed, destructured, initializer, type-only,
same-file, re-exported, and dead cases. Shrike reports the same seven unused
members as knip except `Destructured.Second` (deliberate over-credit) and
credits `Initializer.A` where fallow falsely reports it.

**Consequences:**

- `FileSummary.enums` records each enum's name, ambient/opaque flags, and
  member names, initializer references, and spans; `TSQualifiedName` member
  accesses are recorded as precise member reads.
- The graph emits warnings after reachability, so a dead enum stays a candidate
  and does not also produce member warnings.
- `ANALYSIS_VERSION` is bumped to 75.
- The CLI groups the code under the `types` diagnostic category.

**Revisit if:** Shrike adds a verified enum-member edit, or TypeScript changes
how const-enum and namespace member accesses are emitted.

---

## D-040 — Unused class members are advisory warnings

**Decision:** Report a member of a live, exported class that no project
reference reads as an `unused_class_member` warning. Never emit it as a
deletion candidate: removing a member changes the class contract, and Shrike's
batch deletion model only removes whole symbols. Restrict the rule to top-level
class declarations and `const` class expressions in non-declaration,
non-generated source files whose class symbol is live, exported, and neither
public API nor an entry. Abstain on ambient, abstract, decorated,
computed-name, index-signature, duplicate-declaration, non-React
member-expression heritage, and escaping class or instance values, including
cross-file escapes recorded in `FileSummary.class_value_escapes`. Credit
`this.x`, `super.x`, `ClassName.x`, `new C().x`, `C.prototype.x`, instance
bindings (`const x = new C()`, `x: C`), destructuring, namespace, local and
re-export aliases, literal dynamic imports, and member names shared with an
ancestor. In React projects, credit the lifecycle allowlist and runtime fields
(`state`, `props`, `context`, `refs`, `contextType`, `defaultProps`,
`propTypes`); Next.js additionally credits `getInitialProps`.

**Alternatives considered:**

- Emit a deletion candidate: rejected because a member is not a symbol and the
  edit can change a class contract or a framework call site. The warning names
  the class and member for manual review.
- Report members of dead classes: rejected because the class itself is already
  one deletion candidate; reporting each member would duplicate the finding.
- Follow fallow 3.45.0's exported-binding check: fallow reports members of a
  class whose value is logged with `console.log(Widget)`. Shrike's parser
  records whole-value uses and the graph abstains on the escaped class, which
  avoids the false positive.
- Credit only direct member reads: rejected because `super`, inherited-name
  dispatch, and aliases can reach a member through a path the syntactic access
  does not name. Over-crediting can hide a finding; it can never invent one.
- Run the rule only for React class components: rejected because plain
  TypeScript service and domain classes have the same dead-member shape, and
  the conservative gates keep the false-positive surface small.

**Why:** fallow 3.45.0 detects class members by default; knip 5.88.1 only runs
the check with `--include classMembers`. A probe at
`/tmp/opencode/class-member-probe-2` covers used and unused fields, methods,
accessors, static members, parameter properties, inheritance, local classes,
and a logged React class. Fallow reports 11 unused members and Shrike reports
10: Shrike abstains on two members of `Widget` because `console.log(Widget)`
escapes the class value, and Shrike reports `Base.baseDead` where fallow misses
it because fallow treats `Base` as an unused export even though
`Sub extends Base` keeps it live.

**Consequences:**

- `FileSummary.classes` records each class's members, self/super/static reads,
  and escape flags; `class_bindings` records local instance bindings and
  `class_value_escapes` records whole-value uses for cross-file resolution.
- The graph emits warnings after reachability, so a dead class stays a
  candidate and does not also produce member warnings.
- `ANALYSIS_VERSION` is bumped to 76.
- The CLI groups the code under the `types` diagnostic category.

**Revisit if:** Shrike adds a verified class-member edit, or TypeScript changes
how parameter properties and `useDefineForClassFields` emit fields.

---

## D-041 — Same-file typed React props and local wrappers are harvested

**Decision:** Extend `unused_component_prop` beyond inline destructures. A bare
`props` parameter annotated with a bare same-file `interface` or
`type X = { ... }` object type contributes one prop per property signature, and
`props.<name>` / `const { <name> } = props` reads credit it. Local
`memo(...)` and `forwardRef(...)` wrappers whose first argument is an inline
arrow or function component are candidates too; `forwardRef<Ref, Props>` may
supply the props type through its second generic argument. Abstain on imported,
generic, inherited, union, intersection, qualified, inline-literal, and merged
type declarations; on rest/spread or any other whole-object use of the props
binding; on a `memo` comparator; and on exported or otherwise escaping wrappers.

**Alternatives considered:**

- Keep the inline-destructure-only scope: rejected because fallow 3.45.0
  already harvests same-file typed interfaces and wrappers, so Shrike misses a
  common React/TypeScript shape.
- Resolve imported types or alias chains: rejected because the parser is
  single-file and has no type checker; guessing across modules would widen the
  false-positive surface.
- Follow fallow's exported-binding-only escape check: rejected for the same HOC
  false positive as D-038.

**Why:** fallow's `crates/extract/src/visitor/react.rs` resolves bare same-file
object types in `resolve_typed_react_props`, credits `props.x` and
destructure-from-props, and abstains on whole-object use; its
`classify_component_init` handles `memo`/`forwardRef`. Shrike previously
rejected every bare `props` parameter and did not recognize local wrappers, so
`const Card = memo((props: CardProps) => <div>{props.title}</div>)` produced no
warning even when `tone` was unread.

**Consequences:**

- `ComponentPropRecord.destructured` distinguishes inline destructures from
  typed declarations for message wording.
- The parser collects same-file object types before harvesting, so a type
  declared after the component resolves.
- `ANALYSIS_VERSION` is bumped to 77.
- The `unused-component-prop-1` corpus case covers inline, typed, `memo`, and
  `forwardRef` shapes.

**Revisit if:** Shrike gains a cross-file type resolver, or fallow changes its
typed-props abstention rules.

---

## D-042 — Unused Pages Router data keys are advisory warnings

**Decision:** A Pages Router page whose `getStaticProps` or
`getServerSideProps` statically returns `{ props: { ... } }`, or whose
default-exported component carries a statically returning
`Page.getInitialProps = ...`, reports an `unused_page_data_key` warning for each
returned key the page component never reads. The rule is gated on a live
Next.js Pages Router route entry. It abstains unless the return value and the
page component's props are both statically readable: non-literal `props`
values, spreads, computed or duplicate keys, accessors, multiple or bare
returns, class components, HOCs, `memo`/`forwardRef` wrappers, imported props
types, rest destructures, and whole-object props use all suppress the rule.

**Alternatives considered:**

- Treat every declared prop as consumed: rejected because the page component is
  exported, so `unused_component_prop` abstains on it; a declared-but-unread
  typed prop would then never be reported by any rule.
- Treat `getInitialProps` as a named export: rejected because Next.js reads it
  from the loaded default component, not the module namespace; the existing
  `invalid_pages_router_export` rule already encodes that distinction.
- Model the rule as a deletion candidate: rejected because a returned key is
  not a symbol and removing it changes the page's data contract.
- Cover App Router `pageProps`: rejected because App Router server components
  read route params and search params through framework APIs rather than a
  returned props object; there is no analogous static key list.

**Why:** fallow 3.45.0's `unused_load_data_key.rs` covers SvelteKit, React
Router, and Remix but not Next.js, and knip 5.88.1 has no equivalent. On
`/tmp/opencode/next-page-data-probe` (a page returning `used` and `unused` and
reading only `used`) neither tool reports the key; Shrike now does.

**Consequences:**

- `PageDataKeyRecord` carries the data-function name, key, and key span; the
  graph anchors the warning on the returned property.
- The CLI groups the code under the `next` diagnostic category.
- The graph reuses the Pages Router classifier to exclude `pages/api.ts`, the
  `pages/api/` subtree, and the root-reserved `_app`/`_document`/`_error`
  pages; a nested `_app` or `_error` remains an ordinary page.
- `ANALYSIS_VERSION` is bumped to 78.
- The `unused-page-data-key-1` corpus case covers inline destructuring and
  typed `getInitialProps` while keeping the page and data function protected.

**Revisit if:** Next.js changes the Pages Router data contract, or a competitor
adds a Next.js-specific equivalent with a materially different abstention set.

---

## D-043 — Pages Router route-tree conflicts are error diagnostics

**Decision:** Shrike mirrors Next.js's Pages Router `sorted-routes` tree and
reports only the shapes a real `next build` rejects, as error diagnostics rather
than findings. Sibling dynamic-name conflicts and repeated slug names reuse
`dynamic_segment_name_conflict`; required-plus-optional catch-alls, an optional
catch-all sharing a position with a static page, non-final catch-alls, and
optional non-catch-all segments use the new `pages_router_route_conflict`.

**Alternatives considered:**

- Report every pair of files that resolves to the same URL: rejected because
  `pages/foo.tsx` with `pages/foo/index.tsx`, `pages/api/foo.ts` with
  `pages/api/foo/index.ts`, `pages/[id].tsx` with `pages/[id]/index.tsx`, and
  `pages/foo.tsx` with `pages/foo.js` all build successfully in Next.js 16.4.6.
  Next.js silently chooses one file; claiming a build failure would be false.
- Reuse `route_collision` for every Pages Router failure: rejected because the
  catch-all and specificity errors are not same-URL leaf collisions, and the
  existing `route_collision` message would misdescribe them.
- Add a Pages-specific code for name conflicts too: rejected because the
  failure and the remediation are identical to the App Router
  `dynamic_segment_name_conflict`, so one code keeps the CLI and docs coherent.
- Merge `pages` and `src/pages` into one namespace: rejected because a package
  with both roots builds successfully; Next.js uses one root per package.
- Treat API routes or reserved pages as non-participants: rejected because
  `pages/api/[id].ts` with `pages/api/[slug].ts` fails, while root `_app`,
  `_document`, and `_error` are not sorted routes.

**Why:** fallow 3.45.0's `route_collision` and
`dynamic_segment_name_conflict` cover the App Router only, and knip 5.88.1 has
no route-tree check. Real `next build` probes on Next.js 16.4.6 at
`/tmp/opencode/next-pages-conflict-probe` (`results.txt` and `results-2.txt`)
confirm each failure and each deliberate non-failure. The failures come from
`dist/shared/lib/router/utils/sorted-routes.js`.

**Consequences:**

- `PagesRouterPage` retains the route segments and page root; the graph builds a
  per-package route tree keyed by `(package, root, canonical position)`.
- The checks honor `pageExtensions`, include API routes, exclude root-reserved
  pages, and require the owning package to declare `next`.
- The CLI groups `pages_router_route_conflict` under the `routing` category.
- `ANALYSIS_VERSION` is bumped to 79.
- Graph tests pin the failures, the non-failures, the per-package gate, and the
  `pages`/`src/pages` separation; the `next-pages-router-route-conflict` corpus
  case keeps both page defaults protected and still reports a dead helper.

**Revisit if:** Next.js starts failing on a shape the probes currently accept,
or stops sorting Pages Router routes through `sorted-routes`.

---

## D-044 — Unused TypeScript namespace members are advisory warnings

**Decision:** Report an exported member of a live, exported top-level TypeScript
namespace that no project reference reads as an `unused_namespace_member`
warning. Never emit it as a deletion candidate: removing a member changes the
namespace contract, and Shrike's batch deletion model only removes whole
symbols. The parser abstains on local namespaces, declaration files,
`declare global`, and `declare module "..."`. Merged declarations union their
members, and a dotted `export namespace A.B {}` records `B` as a member of `A`
with dotted descendant paths. A nested namespace that is unused is reported
alone and its descendants are suppressed.

**Alternatives considered:**

- Emit a deletion candidate: rejected because a namespace member is not a
  whole symbol and removing it can break a public contract or a merged
  declaration. The warning names the owner and member for manual review.
- Report members of local namespaces exposed by `export { Ns }`: rejected
  because exporting the namespace object makes every member part of the
  module surface, so no member is individually dead.
- Report members of dead namespaces: rejected because the namespace itself is
  already one deletion candidate; per-member findings would duplicate it.
- Treat a bare type reference (`: Color`) as a whole-namespace use: rejected
  because the type position does not read the runtime object or expose its
  members. Qualified type names still credit their paths, and `typeof Ns`
  credits the whole object.
- Follow a strict "only explicit member reads count" policy: rejected because
  aliases, namespace imports, `import x = Ns.member`, destructuring, computed
  access, inline import types, and unqualified body references can all reach a
  member through a path a simple member-expression walk would miss.
  Over-crediting can hide a finding; it can never invent one.
- Skip ambient namespaces in `.ts` files: rejected because an exported
  `declare namespace` in a `.ts` file is program-visible, yet fallow still
  checks its exported type members. It is an entry, but not a public package
  surface, so its members remain reviewable.

**Why:** fallow 3.45.0 detects unused namespace members by default; knip 5.88.1
has no equivalent. Shrike previously treated the whole namespace as one
symbol, so a live namespace with dead members produced no warning. On fallow's
`tests/fixtures/unused-namespace-members` fixture Shrike reports the same ten
findings as fallow (`Color.unusedColorHelper`, `Legacy.unusedFn`,
`Legacy.unusedValue`, `Merged.unusedHelper`, `Outer.Inner.unusedDeep`,
`Outer.UnusedNested`, `Typed.UnusedShape`, `make.unusedDefaults`,
`Api.Types.UnusedResponse`, and `Dotted.Inner.unusedDotted`), plus
`Api.suppressed`, which fallow hides with a `fallow-ignore-next-line` comment
that Shrike does not yet support.

**Consequences:**

- `FileSummary.namespaces` records each namespace's exported members with
  dotted paths and spans; `namespace_member_uses` records dotted reads,
  whole-object uses (`path: "*"`), and inline import types
  (`object: import("source")`).
- The parser records unqualified references inside a namespace body against
  the root so `Body.entry()` credits the `helper` it reads, and it records only
  the outermost member chain for `Outer.Inner.deep`.
- The graph resolves namespace imports, local and re-export aliases,
  `export * as ns`, literal dynamic imports, and public package re-exports. A
  namespace re-exported from a public entry is skipped as public surface.
- `ANALYSIS_VERSION` is bumped to 80.
- The CLI groups the code under the `types` diagnostic category.
- The `unused-namespace-member-1` corpus case covers whole-object uses,
  spreads, computed access, destructuring, `import =`, `typeof`, inline import
  types, merged declarations, body references, dotted declarations, and the
  local/ambient controls.

**Revisit if:** Shrike adds ignore-comment support, gains a cross-file type
resolver, or TypeScript changes how namespace members are emitted.

---

## D-045 — React.lazy and next/dynamic inline loaders credit only `default`

**Decision:** When an inline loader callback passed to an unshadowed
`React.lazy` or `next/dynamic` call directly returns `import("./Widget")`,
credit only the loaded module's `default` export. The rule covers React's
named `lazy` import (including aliases), React's default or namespace import
used as `.lazy`, and `next/dynamic`'s default import. It accepts an expression
body, a block body, `await`, TypeScript wrappers, conditional and logical
branches, and returns from nested blocks. It does not descend into nested
functions. The callback must return the import itself; a callback that passes
the namespace to another call keeps the widening record, and a `.then(...)`
callback keeps the named credit already recorded by the `.then` collector.
The pass abstains on local wrappers, local lookalikes, shadowed loader
bindings, type-only imports, imports from other modules, and TypeScript
namespace bodies. Local loader functions passed by identifier are narrowed
under D-046.

**Alternatives considered:**

- Keep the whole namespace live: rejected because `React.lazy` and
  `next/dynamic` only read `default`, so every named export in a lazily loaded
  component module was a false negative.
- Treat any callable wrapper as a loader: rejected because an unknown wrapper
  can consume named exports or return the namespace to arbitrary code.
- Trust the callee name without import provenance: rejected because local
  `lazy` helpers, `@loadable/component`, and shadowing parameters have no
  React/Next contract.
- Narrow by identifier for local loader functions in the first change:
  deferred because it needs whole-module reference accounting; the inline form
  is the common React/Next shape and the safe subset. Implemented later as
  D-046.
- Report a diagnostic when the loader returns a named export: rejected because
  the `.then` form is valid and already narrows precisely.

**Why:** fallow 3.45.0's `record_arrow_wrapped_dynamic_import` credits only
`default` for these loaders and its extractor tests pin aliases, namespace
imports, shadowing, type-only imports, and extra arguments. Knip 5.88.1
reports no unused exports for the same probe. Before this change Shrike
reported zero candidates for `unusedLazyNamed` and `unusedDynamicNamed` in
`/tmp/opencode/react-lazy-probe`; after the change both are candidates while
the lazy defaults and the `.then`-selected `NamedView` stay live.

**Consequences:**

- `FileSummary.dynamic` records from a proven loader carry
  `imported: ["default"]`; no new diagnostic or CLI category is added because
  the rule changes liveness, not reporting.
- The parser post-pass tracks lexical scopes, function and block declarations,
  catch parameters, loop bindings, class static blocks, and TypeScript
  namespaces so a shadowed loader name cannot narrow.
- The `react-component-loader-1` corpus case covers React default, namespace,
  and aliased named imports, `next/dynamic`, a `.then` default wrapper, an
  unknown local wrapper, and a shadowed loader parameter.
- `ANALYSIS_VERSION` is bumped to 81.

**Revisit if:** Shrike supports `@loadable/component` or Vue's
`defineAsyncComponent` as first-class loaders, or React/Next changes the loader
contract.

---

## D-046 — Local component-loader functions narrow only when every reference is a loader argument

**Decision:** A top-level `const` arrow or function expression, or a function
declaration, whose body directly returns one or more static `import()`
expressions is a local component loader. When at least one unshadowed reference
to the binding is the first argument of a proven `React.lazy` or
`next/dynamic` call, and every other unshadowed reference is another such
argument, credit only each loaded module's `default` export. Exporting the
loader, declaring it with `let` or `var`, using a computed import specifier, or
referencing it anywhere else keeps every export live. A shadowed reference does
not count toward the reference total, and the binding is not registered when a
same-name declaration already exists at module scope.

**Alternatives considered:**

- Narrow any top-level function with a returned `import()`: rejected because a
  function can also be called directly, passed to an unknown wrapper, or
  exported; only exclusive use as a loader argument proves the React/Next
  contract.
- Narrow when the loader has at least one loader call, ignoring extra uses:
  rejected because the extra use can read named exports or return the
  namespace, as the `/tmp/opencode/react-local-loader-probe` `otherUse` control
  shows.
- Track only the loader binding declaration: rejected because exported and
  reassignable bindings escape the module, and a `let`/`var` loader can be
  replaced before the loader call runs.
- Descend into nested functions to collect returned imports: rejected for the
  same reason as D-045; their `return` statements belong to them.

**Why:** fallow 3.45.0's
`crates/extract/src/visitor/visit_impl_dynamic_imports.rs` resolves a local
loader function passed by identifier and its tests pin local, exported,
reassignable, other-use, and unreferenced cases. On
`/tmp/opencode/react-local-loader-probe`, Shrike after the change reports
exactly `app/page.tsx#otherUse`, `src/LocalDynamic.tsx#unusedLocalDynamicNamed`,
`src/LocalLazy.tsx#unusedLocalLazyNamed`, and
`src/LocalLazy.tsx#NamedLocalLazy`; the shared loader with a `.then` use keeps
`src/Shared.tsx` entirely live. Knip 5.88.1 reports only the unused `react-dom`
dependency on the same probe.

**Consequences:**

- The parser post-pass records every unshadowed identifier reference to a
  registered local loader and every reference credited as a loader argument.
  Narrowing requires `credited > 0` and `references - credited == 0`.
- The loader body accepts direct returns, `await`, TypeScript wrappers,
  conditional and logical branches, sequence expressions, and returns from
  nested blocks, switch cases, `try`/`catch`/`finally`, labels, and loops.
  Function declarations, arrows, and function expressions are supported; the
  import source must be a string literal or an expressionless template literal.
- `FileSummary.dynamic` records from a narrowed local loader carry
  `imported: ["default"]`; no new diagnostic or CLI category is added.
- The `react-component-loader-1` corpus case covers local lazy and dynamic
  loaders plus exported, reassignable, and shared-use controls; the parser
  tests cover non-static and shadowed-reference cases.
- `ANALYSIS_VERSION` is bumped to 82.

**Revisit if:** React/Next starts passing named exports to a loader, Shrike adds
cross-module loader tracking, or a loader reference can be proven safe through
an alias or wrapper.

---

## D-047 — Absent optional React props are advisory warnings

**Decision:** Report an optional prop that a live local React component reads but
no statically known JSX render site supplies as an `absent_component_prop`
warning. Never emit it as a deletion candidate: removing the prop or adding a
call-site value changes the component contract, and the default may be
intentional. The parser records a contract for every top-level uppercase
function or `const` component that returns JSX, including exported components,
with each prop's optionality (a destructuring default or `?` on a same-file
object type member), read status, and default flag. The graph resolves JSX tags
through local bindings, imports, re-export aliases, and barrels, and warns only
when at least one site resolves to the component and every resolved site omits
the prop. The rule abstains when the component value escapes (same-file or
cross-file `memo(Card)`, a registry, `const Alias = Card`), when a namespace
import is rendered as `<UI.Card />` or escapes as a value (`{ ...UI }`,
`const { Card } = UI`), when a runtime-reachable dynamic import targets the
component's module, when any site spreads props or uses a namespaced attribute,
when the component is dead, entry, public API, or generated, and when there is
no known render site. A direct export is not an escape. Only props the component reads are reported, so an unread prop remains
the `unused_component_prop` case. Gate the rule project-wide on a React runtime.

**Alternatives considered:**

- Treat a direct export as an escape: rejected because fallow 3.45.0 follows
  exported components to their known callers, and an exported component in an
  ordinary module still has a closed, resolvable set of in-project render
  sites. Public API and entry surfaces are the shapes that actually open the
  consumer set.
- Treat "no known render site" as absence: rejected because an exported
  component may be consumed outside the analyzed project, and a library
  component legitimately has no in-repo caller.
- Report unread optional props too: rejected because that duplicates
  `unused_component_prop` and changes the question from "no caller supplies
  this" to "the component declares but never reads this".
- Skip props with defaults: rejected because fallow reports them with a
  `has_default` flag and its tests pin the positive case; Shrike names the
  default in the warning so the reviewer can decide whether omission is
  intentional.
- Emit a deletion candidate: rejected because a prop is not a symbol and the
  edit is a contract change outside the batch deletion model.

**Why:** fallow 3.45.0 implements `absent_component_props` in
`crates/core/src/analyze/absent_component_prop.rs` and its integration tests
cover the positive case, explicit supply, spread and namespace uncertainty,
barrel aliases, public and entry surfaces, escapes, and dead modules. Shrike's
existing parser only recorded local non-exported components for
`unused_component_prop`, and the graph had no index of JSX consumers, so it
missed this rule entirely.

**Consequences:**

- `FileSummary.component_contracts` records exported and local component
  contracts; `jsx_uses` records simple JSX tags with supplied attribute names;
  `jsx_namespace_uses` records member-expression roots; and
  `component_value_uses` records non-JSX, non-direct-export value references.
- The graph emits warnings after reachability and before candidate selection,
  so a dead component stays one deletion candidate and no prop warning is
  duplicated.
- The CLI groups `absent_component_prop` under the `react` diagnostic category.
- The `unused-component-prop-1` corpus case includes omitted optional props;
  graph tests cover supplied, spread, namespace, escape, entry/public, dead,
  typed, defaulted, unread, and barrel-alias controls.
- `ANALYSIS_VERSION` is bumped to 84.

**Revisit if:** Shrike gains a consumer graph for components rendered outside
JSX (for example custom renderers), or fallow changes its escape and
public-surface rules.

**2026-11-22 follow-up:** a default export of a local component
(`const Card = ...; export default Card;`) creates a `#default` symbol whose
reference edge points at `#Card`. The absent-prop rule now checks the contract
under both keys, so a default import that resolves to `#default` is compared
with the same component and no longer produces a false negative. The
competitor benchmark's `Folder` case on portfolio is the regression: Shrike now
reports the same 14 absent props as fallow. `ANALYSIS_VERSION` is bumped to 85
together with D-048 and D-049.

---

## D-048 — Returned dynamic imports narrow to the names their callers consume

**Decision:** A top-level function declaration whose every return is a literal
`import("./m")` of the same specifier is a local loader. When every call site in
the same module either destructures the awaited result, reads static members of
the awaited result, or discards it, the import consumes only those names (or no
names for a discard). A bare or opaque call, a computed member read, a
non-awaited call, a `.then`/`.catch`/`.finally` on the returned namespace, an
exported loader, a duplicate declaration, a shadowed loader name, or mixed
returns keeps the whole namespace live. A bare `await import("./m");` or
`import("./m");` expression statement is side-effect-only and consumes no
exports; `void import("./m")` and other value uses still widen.

**Alternatives considered:**

- Narrow only when the loader is immediately awaited at every call site:
  rejected because a caller can legitimately bind the promise or destructure
  after an `await` in a later statement, and those shapes are statically
  visible.
- Treat a non-awaited call as consuming nothing: rejected because the caller
  can pass the promise to arbitrary code, which may read any export.
- Track arrow and function-expression loaders too: rejected because the same
  binding shapes are already covered when they are component loaders
  (D-045/D-046); a general expression loader needs alias and escape accounting
  that the declaration-only rule avoids.
- Descend into nested functions: rejected because their `return` statements
  belong to them, not to the loader.

**Why:** the benchmark found requo test helpers that do
`return import("@/lib/env")` and `return import("@/lib/deployment")`, and
accounteer helpers that do `return await import("@/lib/analytics/client")` and
then destructure a few names. Treating each helper's return as an arbitrary
namespace use kept every export of those modules live. Narrowing to the
destructured names exposed genuine dead exports in accounteer's analytics client
(`clientAnalyticsConfigured`, `currentConsentState`, the
`CONSENT_REOPEN_EVENT` re-export) that knip and fallow also missed, while keeping
the used names live. The same rule reduced false liveness for the deployment
module's re-exported `DeploymentMode` and its internal-only billing types.

**Consequences:**

- `FileSummary.dynamic` records from a narrowed returned loader carry
  `imported: [...]`; a discard carries `imported: []`. No new diagnostic or CLI
  category is added because the rule changes liveness, not reporting.
- The parser post-pass resolves identifier references against the summary's
  free and member reference sets, so a use in a nested scope still blocks
  narrowing. A binding declared more than once, or shadowed by a parameter,
  local, import, catch binding, or nested declaration, abstains.
- `narrows_dynamic_imports_returned_from_local_loaders` and
  `narrows_side_effect_only_dynamic_imports` cover the positive and abstention
  shapes; `abstains_when_a_loader_name_is_shadowed` covers shadowing and
  duplicates.
- `ANALYSIS_VERSION` is bumped to 85.

**Revisit if:** Shrike adds cross-module loader tracking, or a returned loader
can be proven safe through an alias or an exported wrapper.

---

## D-049 — Namespace bindings used only in property assertions narrow to the asserted names

**Decision:** A literal dynamic-import binding whose only uses are
`expect(ns).not.toHaveProperty("name")` or
`expect(ns).toHaveProperty("name")` assertions and static member reads narrows
to the positively asserted names plus the member-read names. A negative
assertion credits nothing; a positive assertion credits each string-literal
name. Any other reference to the binding (a bare use, a computed member, a
different matcher, a non-literal name, or a call argument) keeps the whole
namespace live.

**Alternatives considered:**

- Treat every `expect(ns)` use as an opaque namespace use: rejected because
  `toHaveProperty` reads one property and `not.toHaveProperty` reads none; both
  are statically visible and common in regression tests for removed actions.
- Credit negative assertions: rejected because the assertion proves the export
  is absent, not that it is used.
- Credit every name in a positive assertion chain: rejected because a chain can
  mix matchers and non-literal values; only the direct `toHaveProperty` names
  are proven.

**Why:** accounteer tests use negative property assertions on a dynamically
imported module to pin that removed server actions stay absent
(`expect(mod).not.toHaveProperty("setSideContextAction")`). A generic namespace
use made every export of that module live, hiding genuinely dead exports and
preventing the graph from reporting them. The assertion-only rule is the
narrowest shape that proves which names the test reads.

**Consequences:**

- `narrow_assertion_bindings` runs after the dynamic-import pass and only
  considers records that are still widening (`imported` is `None`) with a
  literal specifier and a binding name.
- `narrows_assertion_only_namespace_bindings` covers negative-only, positive
  plus member-read, opaque matcher, and static member-read shapes.
- `ANALYSIS_VERSION` is bumped to 85.

**Revisit if:** a test framework adds another property-inspection matcher whose
semantics are equally narrow, or Shrike starts tracking assertion libraries
other than the Jest/Vitest `expect` shape.

---

## D-050 — CLI scan JSON is insertion-ordered; receipts stay canonical

**Decision:** `writeJson` serializes command payloads with plain
`JSON.stringify(value, null, 2)`. The scan payload is built in deterministic
order, so output stays stable without rebuilding the object graph with sorted
keys. Receipts, hashes, and on-disk artifacts continue to use canonical
`stableStringify`.

**Alternatives considered:**

- Keep canonicalizing every scan payload: rejected because a 35 MB requo scan
  pays a full recursive copy and sort before printing, and the memory cost is
  visible in the CLI's peak RSS.
- Add a `--no-canonical` flag: rejected because it splits the output contract
  and makes reproducibility depend on the flag.
- Change the receipt path too: rejected because receipt IDs are hashes over
  canonical JSON and must not depend on insertion order.

**Why:** the competitor benchmark's CPU profile put `stableStringify` and
`normalizeValue` at the top of the CLI's self time on requo. Replacing it with
`JSON.stringify` reduced peak RSS on the 1,673-file scan from 409 MB to 339 MB
and left wall time roughly unchanged, showing that the dependency pass, not
serialization, is the remaining CLI overhead. The scan payload is assembled
from ordered arrays and maps, so its JSON is already stable.

**Consequences:**

- `packages/cli/src/output.ts` no longer imports `stableStringify`.
- `packages/receipts` remains the canonical-JSON owner for receipts and
  hashing.
- No schema or output-shape change; only whitespace/key order in command JSON
  could differ, and the builder's order is fixed.
- No `ANALYSIS_VERSION` bump: this is an output-path change, not an analysis
  change.

**Revisit if:** a command payload becomes order-unstable, or a consumer needs
canonical command JSON for hashing.

---

## D-051 — Large scans reuse the engine snapshot and run independent passes concurrently

**Decision:** keep the existing Rust-engine/TypeScript-CLI split, but remove the
per-scan work that does not contribute to analysis:

- discovery compiles include/ignore globs once with `globset`, falling back to
  the reference matcher for unsupported syntax, non-ASCII paths or patterns, and
  semantic edge cases;
- independent finish passes (member, component-prop, internal-only, and SCC)
  run under `rayon::join` and append diagnostics in the original pass order;
- the per-pass edge sort uses `par_sort_by` with the same total order, and the
  reachability BFS uses hash sets instead of sorted-vector membership and does
  not clone the entry list;
- a repository with no enums skips enum-member analysis;
- the analyze response carries a compact `dependencyInput` projection and the
  already-serialized snapshot bytes, so the CLI does not decode and re-encode
  the full snapshot;
- the CLI imports only the command being run, loads `@shrike/verifier` only for
  an executable config, writes snapshot bytes with `writeRawJson`, and
  prefilters dependency candidates with literal substring checks plus shared
  directory and alias caches.

**Alternatives considered:**

- Port the dependency pass or the CLI to Rust: rejected because it moves the
  config-execution boundary and duplicates the resolver; the measured gap was
  in data movement and startup, not in the dependency algorithm.
- Cache the serialized CLI document: rejected because the dependency pass and
  command options can change without changing the engine snapshot.
- Run all finish passes in one flat parallel pool: rejected because
  `rayon::join` keeps the pass boundaries and diagnostic order explicit.
- Replace the stable edge sort with an unstable parallel sort: rejected because
  tie order would become nondeterministic and the output is expected to stay
  byte-identical; `par_sort_by` keeps the same total order.
- Use `globset` unconditionally: rejected because the existing matcher has
  edge-case semantics (trailing `/**`, non-ASCII `?`, unsupported glob syntax)
  that must stay byte-identical; the fallback preserves them.

**Why:** the earlier benchmark showed the CLI, not the engine, was the
large-repository bottleneck: on requo the CLI added ~2.1 s over the warm engine,
mostly decoding, canonicalizing, and re-encoding the 24 MB snapshot and running
the dependency pass. The engine also paid for repeated glob matching and
serial finish passes. The changes target those costs without touching an
analysis rule.

**Consequences:**

- Requo (1,673 files), final paired interleaved medians: CLI 1.773 s cold /
  0.910 s warm; engine 1.505 s cold / 0.417 s warm (isolated cold runs
  1.12–1.16 s); fallow 1.979 s cold / 1.351 s warm. CLI peak RSS fell from
  339 MB to 122 MB warm.
- Accounteer (675 files): CLI 0.796 s cold / 0.493 s warm; engine 0.464 s cold
  / 0.166 s warm; fallow 0.936 s cold / 0.773 s warm.
- Across all five repositories the CLI beats fallow by 1.12–2.22× cold and
  1.48–2.12× warm; the engine beats it by 1.32–16.4× cold and 3.24–30.5× warm.
  The CLI is 8.5–12.5× faster than knip and 9.1–16.6× faster than depcheck
  cold, and 10.8–16.0× faster than knip's cache warm.
- Accuracy is unchanged: engine JSON is byte-identical to the pre-change binary
  on all five benchmark repositories (requo SHA-256 `fca3d3c5…`), CLI parsed
  JSON is identical on all five, and the corpus, differential, and fixture gates
  pass.
- `ANALYSIS_VERSION` is not bumped by the performance work; the dependency
  projection is an additive response field, and the snapshot bytes are the same
  snapshot.

**Revisit if:** the dependency pass becomes the dominant cost again, a command
needs the full snapshot for a non-dependency purpose, or discovery gains glob
semantics that `globset` cannot reproduce.

---

## D-052 — Competitor claims use a paired, validated, multi-scope corpus

**Decision:** keep the competitor benchmark as a repo-owned harness under
`benchmarks/competitors/` and require every published performance claim to come
from it. The protocol follows fallow's public suite (pinned real-world and
synthetic projects, cold and warm caches, 2 warmups, 5 measured runs, medians,
peak RSS, and finding-count validation) and adds:

- paired/interleaved rounds with a rotating start order;
- separate Shrike CLI and engine rows;
- a 12-fixture corpus: five local Next.js apps, five pinned OSS repositories,
  and 1k/5k synthetic projects;
- invalid-run marking: an exit outside the tool's finding convention, or an
  unparseable report, is excluded rather than timed as a result;
- narrow-scope tools (depcheck, ts-prune, ts-unused-exports) reported separately
  from full scans.

**Alternatives considered:**

- Copy fallow's corpus and tools only: rejected because it would not cover
  Shrike's Next.js workload or the narrow-scope competitors users compare
  against.
- Rank every tool in one table: rejected because depcheck and the export-only
  tools do less work; a combined ranking would misrepresent the comparison.
- Report a single best run: rejected because cold/warm and machine load change
  the result; medians over paired rounds are the claim.
- Include failed competitor runs as timings: rejected because an error path is
  not a scan. Fallow's suite already drops failed knip runs for the same reason.

**Why:** the corpus has to be broad enough to catch both performance and
correctness problems. The first full run found a UTF-8 char-boundary panic in
Shrike's HTML module-script scanner on Vite, and knip exited with a config error
on zod, query, and vite, whose config files import unbuilt workspace packages.
A single-repository benchmark would have missed the first and silently reported
the second as a slow run.

**Consequences:** the 2026-11-23 run is recorded in
[`benchmarks/competitor-2026-11-23.md`](../benchmarks/competitor-2026-11-23.md).
The UTF-8 panic is fixed and covered by
`handles_non_ascii_text_at_a_script_prefix_boundary`; Vite now scans instead of
crashing. Knip configurations that fail to load are marked invalid and excluded
from the full-scan comparison. RSS samples now start only after the child has
called `exec` and verify the sampled PID's start time: a pre-exec fork of the
harness, or a reused PID, could each produce a repeated, impossible 137 MB
reading for fast-exiting tools, so those samples are rejected rather than
reported.

**Revisit if:** a competitor changes its report schema or cache semantics, the
corpus stops fitting in a normal benchmark window, or a benchmark claim needs a
workload the suite does not cover.

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
| D-022 | Unused-file findings require no entry, no import, and a dead module | accepted |
| D-023 | Cypress specs and support files are module entries | accepted |
| D-024 | `scripts/` files are entries only when referenced | accepted |
| D-025 | `"use client"` server-only exports are advisory warnings | superseded by D-033 |
| D-026 | App Router route conflicts are error diagnostics | accepted |
| D-027 | Misplaced RSC directives are advisory warnings | accepted |
| D-028 | Mixed client/server barrels are advisory warnings | accepted |
| D-029 | Client/server leak cones are advisory warnings | accepted |
| D-030 | Async arrow actions and inline directives are Server Actions | accepted |
| D-031 | Definitely non-function `"use server"` exports are warnings | accepted |
| D-032 | Invalid route segment configuration is an error diagnostic | accepted |
| D-033 | Invalid client exports are route-only build errors | accepted |
| D-034 | Pages Router build failures are error diagnostics | accepted |
| D-035 | Root-only Next.js conventions require the app root | accepted |
| D-036 | Next.js exports are scoped to the file type that reads them | accepted |
| D-037 | Route handlers get route-level config, not page/layout cache config | accepted |
| D-038 | Unused React component props are advisory warnings | accepted |
| D-039 | Unused TypeScript enum members are advisory warnings | accepted |
| D-040 | Unused class members are advisory warnings | accepted |
| D-041 | Same-file typed React props and local wrappers are harvested | accepted |
| D-042 | Unused Pages Router data keys are advisory warnings | accepted |
| D-043 | Pages Router route-tree conflicts are error diagnostics | accepted |
| D-044 | Unused TypeScript namespace members are advisory warnings | accepted |
| D-045 | React.lazy and next/dynamic inline loaders credit only `default` | accepted |
| D-046 | Local component-loader functions narrow only when every reference is a loader argument | accepted |
| D-047 | Absent optional React props are advisory warnings | accepted |
| D-048 | Returned dynamic imports narrow to the names their callers consume | accepted |
| D-049 | Namespace bindings used only in property assertions narrow to the asserted names | accepted |
| D-050 | CLI scan JSON is insertion-ordered; receipts stay canonical | accepted |
| D-051 | Large scans reuse the engine snapshot and run independent passes concurrently | accepted |
| D-052 | Competitor claims use a paired, validated, multi-scope corpus | accepted |

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
