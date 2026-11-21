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
