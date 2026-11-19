# Shrike 1.0 — Proof-Carrying Cleanup

> **This is the canonical design and engineering brief for Shrike 1.0.** If a
> document, plan, prompt, or implementation disagrees with this file, this file
> wins until it is amended in the same change.
>
> **Status:** implementation complete for local phases 0–4 and distribution
> surfaces; external launch, design-partner, publish, and real-app acceptance
> items are blocked on credentials or participants (see
> [`TRACKER.md`](TRACKER.md))
> **Authored:** 2026-11-20
> **Working directory:** `/home/mark/code/projects/shrike`
> **Working name:** Shrike · **CLI binary:** `shrike` · **Package name:** local
> package is `shrike`; registry availability is blocked in
> [`PLAN-5`](plans/PLAN-5-distribution-and-launch.md)
> **Tracker:** [`TRACKER.md`](TRACKER.md)
> **Start here:** [`plans/PLAN-0-foundations.md`](plans/PLAN-0-foundations.md)

---

## 1. Role and mandate

You are the implementation agent for **Shrike 1.0**, a greenfield application.
You work from this prompt and the linked documents, not from memory, not from a
competitor's implementation, and not from a prior prototype.

You are expected to:

- read the document that owns a subsystem before implementing it;
- write the fixture and the acceptance test with the code, not after it;
- keep [`TRACKER.md`](TRACKER.md) current with evidence;
- prefer a small, verified result over a large, speculative one;
- treat every false positive as a product defect;
- stop and record an open decision when this package does not answer a question;
- record measurements before making performance claims.

You must not:

- add Knip, Fallow, aislop, CodeTruth, Necro, or any other analyzer as a runtime
  dependency;
- copy code from a prior Shrike prototype or from a competitor;
- compete on the breadth of dead-code rules or framework plugins;
- build a bundle optimizer, a slop score, or a generic MCP scanner;
- apply an edit without running the project's own checks against a baseline;
- call an inference API from the analyzer;
- commit directly to `main`;
- claim "verified" when a required check was skipped, timed out, or could not be
  classified.

**Greenfield rule.** Shrike 1.0 starts from an empty repository. No source file,
config, schema, fixture, package name, or cache format from a prior prototype is
a dependency. Prior research and prior product lessons are inputs; prior
implementation is not. If a design idea from the old prototype is reused, it is
rewritten and re-tested here.

---

## 2. One sentence

> **Shrike deletes the code that agents and PRs leave behind — and proves the app
> still builds.**

Everything in this package serves that sentence. The scan is the input. The
verified deletion PR and its receipt are the product.

The one-sentence test: if a proposed feature does not make that sentence more
true, it is out of scope.

---

## 3. The problem

AI-assisted development changed the shape of technical debt.

When an agent writes code, it adds. It adds a helper, a variant, a fallback, a
second data-fetching path, a new component, a new file. It rarely subtracts. The
old caller remains after the rewrite. The old helper remains after the new one
lands. The old file remains after the route is removed. Tests and lint still
pass, because the residue is unreachable, not incorrect.

The result is a codebase that grows faster than it is understood. The developer
who returns to it cannot tell which of the three similar functions is live, so
they preserve all three. The next agent reads all three and writes a fourth.

This is not a style problem. It is a **trust problem**. A dead-code tool that is
wrong once on a Next.js route, a server action, or a dynamically imported
component teaches the team to ignore it forever. So teams keep the code, and the
tax compounds.

The market has responded with more detection: more rules, more plugins, more
languages, more scores. Detection is already cheap. What is missing is the
ability to **act** on detection without risk.

---

## 4. Why now

Three things changed at once:

1. **Agents became the primary author.** Claude Code, Cursor, Codex, and
   OpenCode are normal workflow. The residue is no longer occasional; it is the
   default state of a fast-moving repository.
2. **The build graph became observable.** Next.js 16.4 ships
   `next analyze export`, which exposes the Turbopack module and chunk graph as
   JSON Lines. A static claim can now be checked against what actually ships.
3. **The incumbents stayed read-only or advisory.** Knip and Fallow report.
   aislop scores and fixes a safe subset. CodeTruth and Necro provide
   deletion-safety evidence to agents. Fallow's fix tells you to run build and
   tests yourself. aislop's safe mode explicitly refuses to delete code. Nobody
   owns the **verified deletion PR with a portable receipt** end to end.

That is the opening.

---

## 5. Market gap and differentiation

### 5.1 The gap is no longer "safe deletion"

The earlier version of this thesis claimed that verified deletion was
unoccupied. That is no longer true. CodeTruth and Necro now sit in adjacent
territory:

- **CodeTruth** provides deletion-safety evidence (`safe_to_delete`,
  `likely_dead`, `uncertain`, `used`), an MCP surface, a baseline, and
  cross-repository evidence. It is Python-first with a JS/TS plugin.
- **Necro** provides confidence tiers, evidence chains, a `test-only` verdict,
  and an MCP `verify_removal` operation that applies a removal in a throwaway
  worktree.
- **Fallow** already audits changed files, auto-fixes, and exposes LSP, MCP, and a
  Node API.
- **Knip** owns configuration breadth and has enormous distribution.
- **Vercel** owns the Next.js bundle graph and an agent that optimizes what
  ships.

"Safely delete dead code" is now a crowded sentence. Shrike needs a sharper one.

### 5.2 The sharper wedge

> **Shrike finds what this PR orphaned, deletes it in verified batches, and
> gives you a receipt that proves the app still builds.**

The differentiation is the **combination**, not any single feature:

1. **PR-native orphan delta.** Not "what is unused now" and not "what changed in
   this file." Shrike compares the merge-base graph to the head graph and reports
   what became dead **anywhere**, with the lost witness and the triggering
   change. A changed-file audit cannot produce this. A current-state scanner
   cannot attribute it. An agent-facing safety tool does not make it the product.
2. **A verified deletion PR, not a verdict.** Shrike applies closure-complete
   batches in isolation, runs the project's own checks against a baseline,
   bisects failures, reverts only the culprit, and leaves a reviewable branch
   with one commit per batch. CodeTruth and Necro advise; Shrike operates.
3. **A portable receipt.** Every batch emits a content-addressed JSON artifact
   with graph hashes, witnesses, patches, checks, baseline status, build evidence,
   and outcome. The receipt is the unit of trust, the PR comment, the CI artifact,
   and the audit trail. It is not a score.
4. **Build-graph evidence.** Shrike joins `next analyze export` data to report
   which routes and client bundles actually contain a dead subtree, and to catch
   dynamic edges the static graph missed. The build graph confirms or suppresses;
   it never replaces the source proof.
5. **Rust speed and determinism.** The engine parses and indexes 10,000 files in
   seconds with bounded memory, and produces byte-identical output for identical
   input. That makes the post-agent hook usable and the CI result reproducible.

A competitor can copy one pillar. Copying all five requires becoming an
operations product with a precision corpus, a build-graph join, and a receipt
contract. That is the moat.

### 5.3 What we deliberately do not differentiate on

- rule count;
- language count;
- framework plugin count;
- bundle optimization;
- a cleanliness score;
- agent chat;
- a hosted dashboard.

Those are owned, commoditized, or off-message.

---

## 6. The five pillars

### 6.1 Whole-graph PR orphan delta

Most PR gates report findings in changed files. That structurally hides the most
valuable dead code: code that was live before the PR and dead after it, in files
the PR never touched.

Shrike computes a baseline graph at the merge base and a head graph at the PR
commit, then reports the **reachability that was lost anywhere in the repo**,
with the PR change that orphaned it. Full design:
[`docs/04-whole-graph-delta.md`](docs/04-whole-graph-delta.md).

### 6.2 Verified deletion batches

When a symbol dies, its exclusive dependencies die with it. Shrike computes the
cascade closure, condenses cycles with Tarjan SCCs, partitions the closure into
independent batches, applies each batch in a worktree, runs the project's checks
against a baseline, bisects on failure, and reverts only the offending deletion.
Full design: [`docs/05-verified-deletion.md`](docs/05-verified-deletion.md).

### 6.3 Portable receipt

Every batch, including rejected ones, emits a machine-readable receipt. Receipts
are content-addressed, schema-validated, and reproducible for the same input.
They are the PR comment, the CI artifact, and the audit trail. Full design:
[`docs/05-verified-deletion.md`](docs/05-verified-deletion.md) §6 and
[`schemas/receipt.schema.json`](schemas/receipt.schema.json).

### 6.4 Build-graph evidence

Static reachability says "no path from an entry." The Turbopack snapshot says
"this module is in no client bundle, and this dead subtree accounts for 41 KB
across 6 routes." `next analyze export` is consumed as optional evidence, never
as a hard dependency. It also catches dynamic edges the static graph missed, so
it suppresses false positives rather than creating them. Full design:
[`docs/06-build-graph-and-nextjs.md`](docs/06-build-graph-and-nextjs.md).

### 6.5 Rust speed and determinism

The graph workload holds two whole-repository snapshots, diffs reachability,
condenses cycles, and computes closures. A TypeScript host pays for that in AST
retention and GC. A Rust engine with arena-allocated summaries, interned IDs, and
`rayon` over files gives the product its latency and memory envelope. Full
design: [`docs/01-architecture.md`](docs/01-architecture.md) and
[`docs/03-algorithms.md`](docs/03-algorithms.md).

---

## 7. Buyer, category, and messaging

### 7.1 Category

**Verified cleanup for AI-assisted code.**

Not "vibe coding cleanup." Not "AI slop scanner." Not "dead-code analyzer."

### 7.2 Buyer

**Primary:** the tech lead or staff engineer on a 5–50 person team shipping a
Next.js application with Claude Code, Cursor, Codex, or OpenCode, with a real
build and CI.

They care about:

- not breaking production;
- reviewing a diff they can trust;
- spending minutes, not a day, on cleanup;
- a signal in the PR, not another dashboard.

**Secondary:** an agency doing AI-code cleanup. They need to find the removable
surface quickly, produce evidence for the client, avoid blame for a regression,
and repeat the delivery. They are a beachhead, not the core identity.

**Not the buyer:** the non-technical founder on Lovable/Replit/Bolt. They buy a
done-for-you service, not a CLI. They are a content audience.

### 7.3 Message hierarchy

```text
H1:  Delete what your agent left behind. Prove nothing broke.

Sub: Shrike finds dead code across your whole Next.js repo — including
     the code your last PR orphaned — removes it in verified batches,
     and attaches the build/test receipt.

CTA: Open a verified cleanup PR.

Proof line: Built for the vibe-coding era.
            Trusted by teams that ship.
```

### 7.4 Say / don't say

| Say | Don't say |
|---|---|
| verified cleanup | slop score |
| proof-carrying deletion | vibe cleanliness |
| post-agent cleanup | AI slop detector |
| the cleanup pass | cleanup service |
| AI-assisted code | lines removed |
| verified deletion PR | one-click clean |
| orphan delta | unused code list |

"Vibe coding" is a **hook**, not a home. Use it in launch content, SEO, and
teardowns. Never use it as the category, the H1, or the product name. It attracts
service buyers, is becoming stigmatized, and points the roadmap at scores instead
of verification.

### 7.5 Why this is defensible

- The verifier is an operations product, not a lint rule. Incumbents are
  read-only or advisory by design.
- Whole-graph orphan deltas require two snapshots and a reachability diff; a
  file-scoped audit cannot produce them without changing its architecture.
- Build-graph evidence requires a Turbopack join that a generic analyzer will not
  build.
- Receipts create a track record. A rule can be copied in an afternoon; a
  precision corpus and a history of merged verified PRs cannot.
- The Rust engine plus deterministic output makes the hook and CI path viable at
  a scale where a JS graph implementation stalls.

---

## 8. Product principles

1. **Trust beats breadth.** A narrow tool that is always right is worth more than
   a broad tool that is sometimes wrong.
2. **Proof beats prediction.** The verifier is the authority; the graph is the
   hypothesis.
3. **Fail closed.** A missing fact is not evidence of death. An unknown edge, a
   missing check, an unavailable engine, or an incomparable snapshot blocks
   deletion.
4. **PR first, never auto-commit.** Trust is earned in review. Auto-commit is
   not a v1 feature.
5. **One buyer.** The team with CI. Everyone else is a content audience or a
   design partner.
6. **One sentence.** If a feature does not make the product sentence more true,
   it is out of scope.
7. **Measure, then claim.** No benchmark, precision number, or "verified" claim
   without a reproducible run.
8. **False positives are release blockers.** One false positive on a route,
   server action, or dynamic import is enough to stop a release.
9. **The receipt is the product.** The report is the input; the proof is the
   output.
10. **No slop score.** A score is the wrong metric and commoditizes the product
    into a linter.
11. **Bounded blast radius.** A deletion batch that touches too much of the
    graph is split or refused; the tool never surprises a reviewer with a
    1,000-file diff.
12. **Determinism is a feature.** Same input, same IDs, same output, same
    receipt.

---

## 9. Architecture

### 9.1 Language decision

**Rust engine, TypeScript host, one product.**

| Layer | Language | Owns |
|---|---|---|
| **Engine** | Rust | parsing, semantic summaries, resolution, graph, entries, reachability, witnesses, SCC, delta, cascade, batch partitioning, build-graph join, snapshot cache |
| **Host** | TypeScript | CLI, config execution, project checks, baselines, flaky classification, apply/revert, bisection orchestration, receipts, PR rendering, hooks, agent bundle, MCP wrapper |

Why Rust:

- **Graph workload.** Two snapshots, reachability diff, SCC condensation, and
  cascade closures are memory-and-graph problems. Retaining JavaScript ASTs blows
  the memory budget; arena-allocated Rust summaries do not.
- **Determinism.** Interned IDs and sorted arenas remove the object-identity and
  iteration-order variance that makes JS graph output hard to reproduce.
- **Parallelism.** `rayon` over files with a deterministic merge is
  straightforward.
- **Startup and throughput.** A native binary parses and indexes without a Node
  heap or JIT warmup.

Why the host stays TypeScript:

- `shrike.config.ts` is executable TypeScript; it needs a JavaScript runtime.
- The target project's toolchain is Node; the verifier orchestrates `pnpm`,
  `npm`, `yarn`, and `node`.
- The verifier is process orchestration, not graph computation.
- npm distribution is the natural install path for the buyer.
- The host can evolve without recompiling the engine.

What Rust does **not** buy: type checking, executable config, or product sense.
Do not pretend otherwise.

**Rejected alternatives:**

| Option | Why not |
|---|---|
| TypeScript-only engine | AST retention and GC make two-snapshot diffing and the 10k-file memory budget the wrong place to compete |
| Pure Rust CLI | `shrike.config.ts` and the verifier need Node; embedding a JS runtime adds complexity for no product gain |
| Rust CLI that shells out to Node for everything | Two process boundaries, worse startup, no benefit over a TS host |
| WASM engine in Node | Keeps the JS memory ceiling and adds a boundary |
| Rust engine linked via FFI | Crash risk, tight coupling, and no benefit over a line-delimited process contract |
| A hosted analysis service | Violates the local-first, no-source-upload trust model; a later option only |

### 9.2 Component diagram

```text
┌───────────────────────────────────────────────────────────────┐
│ shrike (TypeScript CLI)                                       │
│                                                               │
│  config ──► checks ──► baseline ──► apply ──► compare         │
│                                      │            │           │
│                                      ▼            ▼           │
│                                   revert      receipt + PR    │
│                                                               │
│  commands: scan, delta, plan, proof, prune, verify, receipt   │
└──────────────────────────┬────────────────────────────────────┘
                           │ versioned JSON contract (stdio)
┌──────────────────────────▼────────────────────────────────────┐
│ shrike-engine (Rust binary)                                   │
│                                                               │
│  loader ──► parse ──► semantic ──► resolve ──► graph          │
│                                      │            │           │
│                                      ▼            ▼           │
│                                   entries ──► reachability    │
│                                      │            │           │
│                                      ▼            ▼           │
│                                   delta ◄──── provenance     │
│                                      │                        │
│                                      ▼                        │
│                              SCC + cascade + batches          │
│                                      │                        │
│                                      ▼                        │
│                              evidence join (optional)         │
└───────────────────────────────────────────────────────────────┘
```

The host owns the command surface and everything that executes project code.
The engine is invoked for analysis and returns structured results. The host
never trusts the engine's exit code alone; it reads the structured result.

### 9.3 Engine contract

The contract is versioned, line-delimited JSON over stdin/stdout.

Request:

```json
{
  "protocolVersion": "1.0",
  "id": "req_01J...",
  "command": "delta",
  "payload": {
    "root": "/repo",
    "base": "origin/main",
    "head": "HEAD",
    "config": {
      "include": ["src/**"],
      "ignore": [".next/**", "node_modules/**"],
      "tsconfig": "tsconfig.json"
    },
    "cacheDir": ".shrike/cache"
  }
}
```

Response:

```json
{
  "protocolVersion": "1.0",
  "id": "req_01J...",
  "ok": true,
  "result": {
    "snapshotHash": "sha256:...",
    "delta": {
      "newlyDeadSymbols": [
        {
          "id": "src/lib/format.ts#formatLegacyDate",
          "reason": "caller removed",
          "trigger": "src/app/invoice/page.tsx"
        }
      ]
    },
    "batches": [
      {
        "id": "batch_1",
        "files": ["src/lib/format.ts"],
        "symbols": ["src/lib/format.ts#formatLegacyDate"],
        "expectedEffect": "Remove 1 dead symbol"
      }
    ]
  },
  "error": null
}
```

Rules:

- one request per line, one response per line;
- a malformed response is a protocol error and fails closed;
- the engine never executes project code, runs checks, or edits the tree;
- the host never edits files outside the worktree or the plan;
- `unknown` edges are returned explicitly; an omitted value is a protocol
  failure, not "no unknown edges";
- a breaking change bumps `protocolVersion` and invalidates cached plans;
- the contract is covered by round-trip fixtures for valid, malformed, timeout,
  engine-crash, and partial-result cases.

Full contract: [`docs/01-architecture.md`](docs/01-architecture.md) §6 and
[`schemas/protocol.schema.json`](schemas/protocol.schema.json).

### 9.4 Data model

All IDs are interned `u32` handles scoped to a snapshot.

| ID | Key |
|---|---|
| `FileId` | normalized relative path |
| `SymbolId` | file, span start, kind, name |
| `ModuleId` | resolved module path |
| `EntryId` | framework entry key (route, export, convention) |
| `EdgeId` | source, target, kind, certainty |

Every edge carries a certainty:

| Certainty | Meaning | Effect on deletion |
|---|---|---|
| `certain` | Resolved import, static reference, framework convention | Normal |
| `probable` | Heuristic match, barrel ambiguity, template prefix | Blocks unless the candidate is in the same SCC and has no external referrer |
| `unknown` | Computed access, unresolved dynamic import, reflection-like pattern | Blocks deletion of every symbol it could reach |

`unknown` edges are a feature. They are how the tool stays honest.

Snapshot model:

```text
GraphSnapshot
  files:      FileId -> FileRecord
  symbols:    SymbolId -> SymbolRecord
  modules:    ModuleId -> ModuleRecord
  entries:    EntryId -> EntryRecord
  edges:      Vec<Edge>
  configHash: string
  resolverHash: string
  toolVersion: string
  protocolVersion: string
```

Full model: [`docs/01-architecture.md`](docs/01-architecture.md) §7 and
[`schemas/graph-snapshot.schema.json`](schemas/graph-snapshot.schema.json).

### 9.5 Caching

| Layer | Key | Stored |
|---|---|---|
| Parse | file content hash + parser version | per-file summary, spans, directives |
| Resolve | importer + specifier + resolver config hash | resolved module ID |
| Snapshot | file hashes + config + resolver + tool version | serialized `GraphSnapshot` |
| Build graph | snapshot hash + Next.js version | joined module map |
| Plan | base/head snapshot hashes + policy | `DeletionPlan` |

Invalidation:

- any file content change invalidates that file and its importers' resolve
  entries;
- a config or resolver change invalidates the whole snapshot;
- a tool or protocol version change invalidates everything;
- a failed parse is cached as a failure with the diagnostic, not retried on
  every run.

Cache lives in `.shrike/cache/` by default, content-addressed, safe to delete,
and excluded from VCS. It never stores source text beyond what a diagnostic
needs.

---

## 10. Folder structure

The repository is a greenfield monorepo. The design lives at the root now; the
implementation directories are created in PLAN-0 and PLAN-1.

```text
.
├── MASTER_PROMPT.md
├── README.md
├── TRACKER.md
├── docs/
├── plans/
├── prompts/
├── research/
├── schemas/
├── fixtures/
├── benchmarks/
├── crates/
│   ├── shrike-core/
│   ├── shrike-syntax/
│   ├── shrike-resolve/
│   ├── shrike-graph/
│   ├── shrike-delta/
│   ├── shrike-plan/
│   ├── shrike-evidence/
│   ├── shrike-receipt/
│   ├── shrike-protocol/
│   ├── shrike-engine/
│   └── xtask/
├── packages/
│   ├── cli/
│   ├── verifier/
│   ├── receipts/
│   ├── agent/
│   ├── action/
│   ├── engine-*/
│   └── testkit/
├── scripts/
└── .github/workflows/
```

Full rationale and ownership: [`docs/02-folder-structure.md`](docs/02-folder-structure.md).

---

## 11. Algorithms

The full algorithm document is [`docs/03-algorithms.md`](docs/03-algorithms.md).
The summary:

```text
1. Inventory files, hash contents, load config.
2. Parse each file with oxc; build a per-file semantic summary.
3. Resolve imports, re-exports, aliases, workspaces, and package exports.
4. Build the module graph and the symbol graph.
5. Seed entries from framework adapters (Next.js, React, packages, tests).
6. BFS from all entries; store parent pointers for witnesses.
7. Mark live symbols; the rest are candidates.
8. Snapshot and cache.
9. Build base and head snapshots; assert comparability.
10. Delta = live(base) − live(head); attribute the lost witness.
11. Tarjan SCC condensation.
12. Cascade closure with exclusive ownership and stop conditions.
13. Estimate impact: files, lines, routes, bytes.
14. Build the conflict graph; partition into batches; enforce blast radius.
15. Materialize patches; verify each batch in a worktree.
16. Bisect failures; revert the culprit; retry the remainder.
17. Emit receipts; open a draft PR.
```

Key algorithm properties:

- **Provenance-carrying reachability.** Every live symbol keeps a shortest
  witness path; every dead symbol has a proof trace listing the edges considered
  and the unknown edges that would block deletion.
- **Dynamic-edge abstraction.** Literal imports are certain; template prefixes
  become globs; computed expressions become `unknown` and block.
- **SCC condensation.** A cycle is a unit. Either the whole SCC is dead or none
  of it is.
- **Cascade closure.** A dependency joins the deletion set only if every live
  referrer is already in the set. Stop at an entry role, public API marker,
  external referrer, or unknown edge.
- **Batch partitioning.** Conflict graph + deterministic greedy coloring, capped
  at 50 symbols or 10 files per batch (configurable), with a blast-radius guard
  that refuses a batch touching more than 5% of the live graph.
- **Bisection.** Deterministic split by `SymbolId`, respecting SCCs. The same
  failure produces the same rejection set.
- **Impact preview.** Before applying, report the cascade closure and estimated
  lines/files/routes/bytes so a reviewer can decide whether to run it.

Complexity: reachability, Tarjan, and closure are linear in the graph; the
snapshot build dominates. Batch coloring is NP-hard in general, so it is a
deterministic greedy heuristic with a hard batch cap.

---

## 12. CLI surface

```bash
shrike scan                          # evidence report (read-only)
shrike proof <symbol>                # witness path or proof of unreachability
shrike delta --base origin/main      # whole-graph PR orphan delta
shrike plan --base origin/main       # deletion batches + impact preview
shrike prune --base origin/main \
  --verify --receipt                 # apply, verify, emit PR + receipt
shrike verify --plan <id>            # re-run verification for a plan
shrike receipt <id>                  # show a receipt
shrike evidence --build <snapshot>   # join Next.js build evidence
shrike hook install --claude         # post-agent cleanup hook
shrike hook install --cursor
shrike hook install --opencode
```

Exit codes:

| Code | Meaning |
|---|---|
| 0 | success / verified / clean |
| 1 | findings or delta above threshold |
| 2 | configuration or usage error |
| 3 | verification failed, changes reverted |
| 4 | verification could not run (fail-closed) |
| 5 | revert failed (highest severity) |

The full command contract is in [`docs/07-agent-and-distribution.md`](docs/07-agent-and-distribution.md)
and [`docs/05-verified-deletion.md`](docs/05-verified-deletion.md).

---

## 13. Receipt

The receipt is the durable proof of a verified deletion. It is written for every
batch, including rejected ones.

```json
{
  "schemaVersion": "1.0",
  "id": "rcpt_01J...",
  "createdAt": "2026-11-20T00:00:00.000Z",
  "tool": { "name": "shrike", "version": "1.0.0" },
  "base": { "ref": "origin/main", "commit": "abc123", "graphHash": "sha256:..." },
  "head": { "commit": "def456", "graphHash": "sha256:..." },
  "plan": { "id": "plan_01J...", "hash": "sha256:..." },
  "build": {
    "source": "next analyze export",
    "snapshotHash": "sha256:...",
    "version": "16.4.0",
    "status": "full"
  },
  "batch": {
    "id": "batch_1",
    "symbols": ["src/lib/format.ts#formatInvoice"],
    "files": ["src/lib/format.ts", "src/lib/currency.ts"],
    "linesRemoved": 412,
    "patches": [".shrike/patches/batch_1.patch"]
  },
  "checks": [
    {
      "name": "typecheck",
      "command": "pnpm typecheck",
      "status": "pass",
      "baselineStatus": "pass",
      "durationMs": 812,
      "reruns": 0
    }
  ],
  "outcome": "verified",
  "rejected": [],
  "environment": {
    "node": "v24.0.0",
    "packageManager": "pnpm@12.18.0",
    "platform": "linux"
  }
}
```

Receipts are written to `.shrike/receipts/<id>.json` and posted as a PR comment.
The ID is derived from the base, head, plan, batch, and check configuration;
`createdAt` is metadata. Re-running the same plan on the same head reproduces the
ID. The serialized file is not byte-identical because it contains the timestamp.

Full schema: [`schemas/receipt.schema.json`](schemas/receipt.schema.json). Design:
[`docs/05-verified-deletion.md`](docs/05-verified-deletion.md) §6.

---

## 14. Verification contract

> **No automated edit is considered successful until the project's configured
> checks pass against a baseline. A failed edit is reverted, never left
> half-applied.**

```text
baseline → apply → check → compare → revert-or-keep
```

Stages:

1. **Baseline.** Capture HEAD, branch, dirty state, exact commands, environment,
   exit codes, durations, failure signatures, flaky classification, and tool
   versions. The baseline is content-addressed and valid for one head commit and
   one check configuration.
2. **Apply.** Default `worktree` (`git worktree add --detach`); optional
   `in-place` with a clean tree and explicit `--allow-dirty`. Snapshot every
   touched file.
3. **Check.** Tiers: fast (typecheck + lint), standard (+ test), full (+ build).
   A check is a command, cwd, timeout, required flag, and rerun count.
4. **Compare.** Against the baseline, not zero. Classify `pass`, `improvement`,
   `known-failure`, `new-failure`, `fail`, `timeout`, `flaky`.
5. **Revert or keep.** Keep verified batches; bisect and revert failures; exit 5
   on revert failure with a recovery snapshot and restore command.

The tree is never left half-applied.

Full contract: [`docs/05-verified-deletion.md`](docs/05-verified-deletion.md).

---

## 15. Build-graph evidence

`next analyze export` is optional evidence. It is consumed, never required.

| Build result | Evidence |
|---|---|
| Present in a client chunk on a live route | **Suppress** the deletion |
| Present only in a server chunk | Weaken; server-only dead code still needs source proof |
| Absent from every chunk | **Confirm** |
| Unjoined module | Unknown; do not confirm |

The join indexes Turbopack modules by normalized source path, then by module ID.
It tolerates additive schema changes, streams JSON Lines, and degrades cleanly
when the snapshot is missing, stale, unsupported, or unparseable. Build edges
that the static graph missed are added as `certain` edges and suppress the
candidate.

Full design: [`docs/06-build-graph-and-nextjs.md`](docs/06-build-graph-and-nextjs.md).

---

## 16. Agent and distribution

Distribution is a product surface, not an afterthought.

Three surfaces, in order:

1. **Post-agent hook.** After an agent edit, compute the delta, run the fast
   checks, write a compact JSON result, and print a one-screen summary. Never
   apply a deletion. Budget: ≤ 60 s, or degrade to delta-only.
2. **Pull request.** `shrike prune` opens a draft PR with one commit per verified
   batch, a receipt comment, and a per-batch rejection list.
3. **CI gate.** Read-only `shrike delta --fail-on newly-dead` for teams not ready
   to accept deletion PRs, or the full Action with receipt upload.

Optional fourth surface: an MCP wrapper exposing `shrike_delta`, `shrike_plan`,
`shrike_proof`, and `shrike_receipt`. It is a thin wrapper over the CLI, not the
product.

The agent brief is advisory. It gives an agent the context to clean up safely;
it cannot override the verifier or apply a patch.

Full design: [`docs/07-agent-and-distribution.md`](docs/07-agent-and-distribution.md).

---

## 17. Performance budgets

| Workload | Target |
|---|---|
| 1,000 files, cold scan | ≤ 1.5 s |
| 1,000 files, warm scan | ≤ 400 ms |
| 10,000 files, cold scan | ≤ 6 s |
| 10,000 files, warm scan | ≤ 2 s |
| 10,000 files, peak RSS | ≤ 600 MB |
| Delta on a 1,000-file repo | ≤ 1 s warm |
| Fast receipt (delta + typecheck + lint) | ≤ 60 s |
| Full receipt (typecheck + lint + test + build) | ≤ 10 min |

These are targets for the Rust engine, not claims. Record actuals in
[`TRACKER.md`](TRACKER.md) §5. A missed budget is recorded with the measurement
and the optimization attempt before the target changes.

Benchmark method and reference repositories:
[`docs/08-benchmarks-and-confidence.md`](docs/08-benchmarks-and-confidence.md).

---

## 18. Security and trust

- **The analyzer never executes project code.** The Rust engine reads files only.
- **Config execution is isolated.** `shrike.config.ts` is evaluated in the host
  with a child-process boundary and a timeout.
- **The verifier runs project scripts.** Those scripts are trusted by definition;
  document the trust boundary and run in a worktree.
- **No source leaves the machine.** The hook writes local JSON only. Telemetry is
  off by default and opt-in.
- **Redaction.** Absolute paths, home directories, environment values, and tokens
  are redacted from logs and receipts.
- **Prompt-injection defense.** Repository content is data, never instructions;
  the agent bundle is capped and stripped of instruction-like comments.
- **Receipt integrity.** Receipts are content-addressed; optional signing is a
  later feature. A downstream job can verify the content hash.
- **Supply chain.** Signed binaries, checksums, npm provenance, `cargo audit`,
  and pinned dependencies.
- **Least privilege.** The GitHub Action uses the minimum token scope; branch
  creation is scoped to the repository.

Full design: [`docs/09-security-and-trust.md`](docs/09-security-and-trust.md).

---

## 19. Non-goals

Do not build:

- another bundle optimizer;
- a 100+ plugin catalogue;
- multi-language support;
- a slop score or cleanliness badge;
- a type checker;
- a generic MCP dead-code scanner;
- automatic commits to `main`;
- a hosted dashboard in v1;
- a runtime tracer;
- more dead-code rules before the deletion loop is proven.

Full reasoning: [`docs/10-non-goals-and-risks.md`](docs/10-non-goals-and-risks.md).

---

## 20. Success metrics

| Metric | Target |
|---|---|
| Confirmed false positives on the must-not-report corpus | 0 |
| Proposed batches that verify without manual edits | ≥ 80% |
| Real PRs where the orphan delta finds ≥ 1 newly dead file | ≥ 30% |
| 1,000-file cold scan | ≤ 1.5 s |
| 1,000-file warm scan | ≤ 400 ms |
| 10,000-file cold scan | ≤ 6 s |
| 10,000-file warm scan | ≤ 2 s |
| 10,000-file peak RSS | ≤ 600 MB |
| Fast path (delta + typecheck + lint) | ≤ 60 s |
| Time to receipt on a 1,000-file Next.js app (full checks) | ≤ 10 min |
| Design partners who merge a Shrike PR in month one | ≥ 3 |
| Receipts with a complete check record | 100% |

---

## 21. Definition of done for v1.0

- [ ] `shrike delta --base origin/main` works on a real Next.js 16 app and
      reports newly dead code in untouched files.
- [ ] `shrike prune --base origin/main --verify --receipt` produces a draft PR
      with independent verified batches and a receipt.
- [ ] A deliberately broken deletion is detected, bisected, reverted, and
      recorded without leaving the tree dirty.
- [ ] The must-not-report corpus is green: routes, server actions, client
      boundaries, dynamic imports, proxy/middleware, metadata files.
- [ ] Build-graph evidence is joined on a real app and reports byte impact.
- [ ] The hook runs after an agent edit and reports the fast-path delta.
- [ ] Benchmarks are recorded in the tracker against the budgets above.
- [ ] One external design partner has merged a Shrike PR.
- [ ] The Rust engine is the only analysis engine; there is no TypeScript
      fallback to maintain.
- [ ] The receipt schema validates every receipt in the fixture suite.

---

## 22. Documentation index

### 22.1 Design documents

| Document | Purpose |
|---|---|
| [`docs/00-thesis-and-positioning.md`](docs/00-thesis-and-positioning.md) | Problem, evidence, buyer, category, messaging |
| [`docs/01-architecture.md`](docs/01-architecture.md) | Rust engine, TypeScript host, contract, data model, caching |
| [`docs/02-folder-structure.md`](docs/02-folder-structure.md) | Repository layout, crate/package ownership, naming |
| [`docs/03-algorithms.md`](docs/03-algorithms.md) | Full pipeline, reachability, witnesses, SCC, cascade, batches, bisection |
| [`docs/04-whole-graph-delta.md`](docs/04-whole-graph-delta.md) | Base/head snapshots, orphan delta, trigger attribution |
| [`docs/05-verified-deletion.md`](docs/05-verified-deletion.md) | Verifier contract, batches, baseline, receipts, exit codes |
| [`docs/06-build-graph-and-nextjs.md`](docs/06-build-graph-and-nextjs.md) | Turbopack ingestion, Next.js semantics, evidence join |
| [`docs/07-agent-and-distribution.md`](docs/07-agent-and-distribution.md) | Hooks, GitHub Action, PR flow, agent protocol, MCP |
| [`docs/08-benchmarks-and-confidence.md`](docs/08-benchmarks-and-confidence.md) | Must-not-report corpus, calibration, public benchmarks |
| [`docs/09-security-and-trust.md`](docs/09-security-and-trust.md) | Trust boundaries, sandboxing, redaction, supply chain |
| [`docs/10-non-goals-and-risks.md`](docs/10-non-goals-and-risks.md) | What not to build, risks, falsification tests |
| [`docs/11-market-research.md`](docs/11-market-research.md) | Competitor matrix, data points, sources |
| [`docs/12-decision-log.md`](docs/12-decision-log.md) | Accepted and rejected decisions with rationale |

### 22.2 Plans

| Plan | Depends on | Exit gate |
|---|---|---|
| [`plans/PLAN-0-foundations.md`](plans/PLAN-0-foundations.md) | — | Contract, schemas, fixtures, CI green |
| [`plans/PLAN-1-rust-engine.md`](plans/PLAN-1-rust-engine.md) | 0 | Engine builds, fixtures pass, budgets met |
| [`plans/PLAN-2-graph-and-delta.md`](plans/PLAN-2-graph-and-delta.md) | 1 | Orphan delta correct on fixtures; corpus clean |
| [`plans/PLAN-3-verification-and-receipts.md`](plans/PLAN-3-verification-and-receipts.md) | 1, 2 | End-to-end verified deletion PR + receipt |
| [`plans/PLAN-4-build-graph-and-nextjs.md`](plans/PLAN-4-build-graph-and-nextjs.md) | 1, 3 | Build evidence joined; byte impact reported |
| [`plans/PLAN-5-distribution-and-launch.md`](plans/PLAN-5-distribution-and-launch.md) | 3, 4 | Hook + Action + 3 design partners |
| [`plans/PLAN-6-hardening-and-scale.md`](plans/PLAN-6-hardening-and-scale.md) | 2–5 | 1.0 release: corpus, security, scale, docs |

### 22.3 Prompts

Cast overview, usage order, and the non-negotiable rules for every role:
[`prompts/README.md`](prompts/README.md).

| Prompt | Role |
|---|---|
| [`prompts/00-orchestrator.md`](prompts/00-orchestrator.md) | Master implementation agent; phase sequencing and definition of done |
| [`prompts/01-rust-engine-agent.md`](prompts/01-rust-engine-agent.md) | Rust crates, parser, resolver, graph, delta, performance |
| [`prompts/02-fixture-and-corpus-agent.md`](prompts/02-fixture-and-corpus-agent.md) | Fixtures, must-not-report corpus, delta and verification cases |
| [`prompts/03-verifier-agent.md`](prompts/03-verifier-agent.md) | Baseline, checks, apply/revert, bisection, receipts |
| [`prompts/04-review-agent.md`](prompts/04-review-agent.md) | Independent review for correctness, security, false positives |
| [`prompts/05-research-agent.md`](prompts/05-research-agent.md) | Competitor and technical research; updates research/ |
| [`prompts/06-release-agent.md`](prompts/06-release-agent.md) | Packaging, binaries, npm, Action, signing, release |
| [`prompts/07-design-partner-triage-agent.md`](prompts/07-design-partner-triage-agent.md) | Trial runs, false-positive triage, corpus feedback |
| [`prompts/08-adversarial-red-team-agent.md`](prompts/08-adversarial-red-team-agent.md) | Break the analyzer with dynamic and framework patterns |

### 22.4 Research

| Document | Purpose |
|---|---|
| [`research/README.md`](research/README.md) | Research index, confidence levels, how to update |
| [`research/00-methodology.md`](research/00-methodology.md) | Sources, dates, evidence standards |
| [`research/01-competitive-landscape.md`](research/01-competitive-landscape.md) | Knip, Fallow, aislop, CodeTruth, Necro, Vercel, others |
| [`research/02-technical-substrate.md`](research/02-technical-substrate.md) | oxc, Rust crates, TypeScript 7, resolution, caching |
| [`research/03-nextjs-build-graph.md`](research/03-nextjs-build-graph.md) | `next analyze export`, Turbopack, client-reference manifest |
| [`research/04-prior-art-and-academia.md`](research/04-prior-art-and-academia.md) | Dead-code analysis, slicing, debloating, delta debugging |
| [`research/05-market-signals.md`](research/05-market-signals.md) | GitClear, Sonar, SD Times, pricing, demand |
| [`research/06-sources.md`](research/06-sources.md) | Full source list with dates and URLs |

### 22.5 Schemas

| Schema | Purpose |
|---|---|
| [`schemas/protocol.schema.json`](schemas/protocol.schema.json) | Engine/host request and response envelope |
| [`schemas/graph-snapshot.schema.json`](schemas/graph-snapshot.schema.json) | Snapshot model |
| [`schemas/delta.schema.json`](schemas/delta.schema.json) | Orphan delta output |
| [`schemas/deletion-plan.schema.json`](schemas/deletion-plan.schema.json) | Batches, impact, blast radius |
| [`schemas/receipt.schema.json`](schemas/receipt.schema.json) | Verified deletion receipt |
| [`schemas/config.schema.json`](schemas/config.schema.json) | `shrike.config.ts` shape |

---

## 23. Execution order

The plans are sequential, but each ships a usable increment.

```text
PLAN-0  Foundations
  contract, schemas, fixtures, corpus, CI, benchmark harness

PLAN-1  Rust engine
  parse, resolve, graph, entries, reachability, witnesses, cache, protocol

PLAN-2  Graph and delta
  base/head snapshots, orphan delta, trigger, SCC, cascade, batches, impact

PLAN-3  Verification and receipts
  baseline, checks, worktree, compare, bisection, revert, receipt, PR

PLAN-4  Build graph and Next.js
  snapshot ingestion, join, byte impact, dynamic feedback, framework depth

PLAN-5  Distribution and launch
  hooks, Action, MCP, agent bundle, landing test, design partners

PLAN-6  Hardening and scale
  corpus expansion, calibration, performance, security, release
```

**Timebox rule:** if the engine work threatens the delta or verification
milestones, cut engine scope before cutting the product loop. The wedge is the
verified PR, not the parser.

---

## 24. Working agreement

1. **Read first.** No subsystem is implemented before its document is read and,
   if it is wrong, amended.
2. **Fixture first.** Every rule, edge case, framework convention, delta
   scenario, and verification path gets a fixture before it gets a release.
3. **Proof first.** No deletion lands without a baseline and an after-check.
4. **Fail closed.** Unknown dynamic edge, missing check, unavailable engine,
   incomparable snapshot, unparseable build snapshot → do not delete; record why.
5. **One product sentence.** If a proposed feature does not make "delete what the
   agent left behind and prove nothing broke" more true, it is out of scope.
6. **No silent scope creep.** Update this document, the relevant doc, and the
   tracker in the same change.
7. **False positives are defects.** One false positive on a route, server
   action, or dynamic import is a release blocker.
8. **Measure, then claim.** Record actual numbers in the tracker. Do not put a
   benchmark in the README that the tracker cannot reproduce.
9. **One plan at a time.** Do not start PLAN-N+1 until PLAN-N's exit gate is
   demonstrated, unless the plan explicitly allows parallelism.
10. **Evidence over assertion.** A phase is done when the exit gate is
    demonstrated and the evidence is linked from the tracker, not when the code
    compiles.

---

## 25. Open decisions

These are tracked and owned by a plan. Resolved items state the decision; every
other item must not be silently decided in code. Blocked items require an
external app, credential, or participant and are not silently marked done.

| ID | Question | Resolution |
|---|---|---|
| Q-001 | Does the Rust engine own the CLI? | Resolved: no. The Rust engine owns analysis; the TypeScript host owns the CLI, config, checks, receipts, and PR flow. |
| Q-002 | Worktree by default, or in-place with snapshots? | Resolved: worktree by default; in-place is explicit and requires a clean tree unless `verification.allowDirty`. |
| Q-003 | How are public-package exports marked? | Resolved: package entry/exports, `.d.ts` files, and configured `publicApi` globs are public; their exports seed entries and are never deletion candidates. |
| Q-004 | Which check tiers are required vs advisory? | Resolved: fast = typecheck + lint, standard = + test, full = + build; default standard. Configured checks replace discovery and are required unless `required: false`. |
| Q-005 | Does the receipt sign with a key, or only content-address? | Resolved: content-address first (SHA-256 in this implementation). Signing is deferred and optional when added. |
| Q-006 | How is a design partner's repo isolated during a trial? | Resolved: the trial kit is read-only, makes no network calls, writes only `.shrike/`, and runs no project scripts. Prune uses a worktree and the partner's checks. |
| Q-007 | Is the package name `shrike` available, or is a rename needed? | Blocked: npm name availability and publish dry-run require registry credentials. The local tarball gate passes. |
| Q-008 | What is the minimum Next.js version for build-graph evidence? | Resolved: 16.4+ is full; 16.1–16.3 is partial; older versions are unsupported and degrade to static analysis. |
| Q-009 | Is the snapshot format stable across minor versions? | Resolved: the protocol/snapshot `1.0` contract is checked; a mismatch fails closed and invalidates the cache. Additive tolerance applies to build evidence, not the engine snapshot. |
| Q-010 | How are monorepo workspace boundaries represented? | Resolved: workspace packages are discovered from `package.json` workspaces and `pnpm-workspace.yaml`; package entries and exports resolve across workspaces. |
| Q-011 | Persistent engine process or one shot per command? | Resolved: one shot per command. Each request is a fresh process with a timeout; there is no persistent daemon. |
| Q-012 | Should test-only and Storybook entries count by default? | Resolved: yes under the default local policy (`testEntries: true`); production policy excludes test-only entries. |
| Q-013 | How is a public API detected outside `package.json` exports? | Resolved: configured `publicApi` globs, declaration files, and a package entry surface. No broader inference. |
| Q-014 | Is the `next analyze export` schema documented per version? | Resolved: no per-version schema is assumed. The parser tolerates additive fields and records unknown record families; the version guard decides capability. |
| Q-015 | How are Turbopack virtual modules represented? | Resolved: loader, `turbopack://`, `file://`, absolute, and query forms normalize to source paths; unjoined virtual modules stay `unknown` and never confirm deletion. |
| Q-016 | Does Shrike run `next build` itself, or only consume a snapshot? | Resolved: it consumes a snapshot by default; `--run-build` is opt-in and evidence is never required. |
| Q-017 | Which hook event is right in Claude Code, Cursor, and OpenCode? | Resolved: Claude `Stop`, Cursor `afterFileEdit` + `stop`, OpenCode `session.idle`. |
| Q-018 | Does the Action need a GitHub App or a token? | Resolved: no App. It uses the workflow's `GITHUB_TOKEN` through `git`/`gh`; prune refuses fork PRs. |
| Q-019 | Per-edit or task-end hook? | Resolved: task-end is the default; Cursor also gets `afterFileEdit` for fast feedback. The hook is advisory, non-blocking, and never applies a deletion. |
| Q-020 | How large must the labeled set be before publishing precision? | Blocked: 10–20 labeled real repositories are required; no precision number is published before that. |
| Q-021 | Publish per-repository results or only aggregates? | Deferred: receipts stay local/CI until the labeled set exists; publication then requires partner consent. |
| Q-022 | How are design-partner labels kept confidential? | Resolved for the trial: no source upload, local reports, and a private triage template. Aggregate publication requires explicit permission. |
| Q-023 | Native binary or WASM? | Resolved: native first. WASM remains a fallback if native distribution fails. |
| Q-024 | Which platforms are supported at first? | Planned: Linux x64/arm64, macOS x64/arm64, Windows x64. The local packaging gate covers the launcher; registry publication is blocked. |
| Q-025 | How is a feature removal distinguished from a rename? | Resolved: conservatively. The delta reports only symbols present in both graphs that were live at base and are dead at head; a removed or renamed symbol is not a deletion candidate, so a rename cannot become a false orphan. |
| Q-026 | How are affected-only tests selected? | Deferred: start with full project checks. Affected-only selection is added only after measuring real check cost. |
| Q-027 | Is the client-reference manifest available across Next.js versions? | Resolved: it is optional evidence; when present it adds `certain` build edges, and when absent static reachability remains authoritative. |
| Q-028 | Pricing and packaging | Resolved for v1: free CLI and design-partner trials; no hosted product or paid tier. |
| Q-029 | How is the blast-radius threshold configured? | Resolved: `batchPolicy.maxSymbols` (50), `maxFiles` (10), `maxLiveGraphPercent` (5), and `allowOversized` (false) in config; oversized batches split or refuse. |
| Q-030 | Do receipts include a human-readable summary and a machine JSON? | Resolved: every batch writes schema-validated JSON under `.shrike/receipts/`; the PR body and per-receipt comments are the human Markdown summaries. |

---

## 26. What would change our mind

- If a strong incumbent makes verified deletion free, reliable, and receipted,
  the wedge moves to build-graph attribution or the agency workflow.
- If `next analyze export` disappears, the build-graph pillar weakens; the
  product becomes static analysis plus verification.
- If the orphan delta is mostly noise on real PRs after triage, rework the
  abstraction before scaling.
- If verification takes more than 15 minutes on a typical app, tiered and
  affected-only checks become the product, not an optimization.
- If design partners prefer a report or score over a verified PR, the buyer is
  wrong and the positioning must be revisited.
- If the Rust engine cannot hit the budgets after two documented optimization
  passes, reduce scope; do not silently keep a TypeScript engine as the product.
- If native binaries cannot be distributed on the design partners' CI platforms,
  evaluate WASM or a hosted analyzer before changing the product surface.
- If the buyer turns out to be agencies rather than teams, the packaging becomes
  a report generator with a verification core.

---

## 27. Start here

1. Read [`plans/PLAN-0-foundations.md`](plans/PLAN-0-foundations.md).
2. Load [`prompts/00-orchestrator.md`](prompts/00-orchestrator.md).
3. Create the repository scaffold and the contract.
4. Do not write parser code until the contract, schemas, and fixtures are green.
5. Record every completed task in [`TRACKER.md`](TRACKER.md) with a link to the
   evidence.

The product is the verified deletion PR. Build that, not a dashboard.
