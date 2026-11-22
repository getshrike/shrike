# 00 — Thesis and Positioning

> **Parent:** [MASTER_PROMPT.md](../MASTER_PROMPT.md) · **Tracker:** [TRACKER.md](../TRACKER.md)
> **Status:** canonical · Authored 2026-11-20

---

## 1. The problem

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

## 2. Why now

Three things changed at once:

1. **Agents became the primary author.** Claude Code, Cursor, Codex, and
   OpenCode are normal workflow. The residue is no longer occasional; it is the
   default state of a fast-moving repository.
2. **The build graph became observable.** Next.js 16.4 ships
   `next analyze export`, which exposes the Turbopack module and chunk graph as
   JSON Lines. A static claim can now be checked against what actually ships.
3. **The incumbents stayed read-only or advisory.** Knip and Fallow report.
   aislop scores and fixes a safe subset. CodeTruth and Necro give agents
   deletion-safety evidence. Fallow's fix tells you to run build and tests
   yourself. aislop's safe mode explicitly refuses to delete code. Nobody owns
   the verified deletion PR with a portable receipt end to end.

That is the opening.

---

## 3. Market evidence

### 3.1 The pain is measured

| Signal | Evidence |
|---|---|
| Duplication is rising | GitClear 2026 (623M changes): block duplication 40.3 in 2023 → 73.0 YTD 2026, +81%, highest on record |
| Churn is rising | GitClear: two-week churn ~16% in 2023–2024 → ~19% in 2025–2026; AI power users churn at 9× |
| Trust is low | Sonar: 53% attribute negative technical-debt impact to AI code that looked correct but was not reliable |
| Cleanup is paid for | Services charge $299 (Upwork low) to $40k; ~$7,200 average completed invoice; $2k–$10k audits; $500–$2k/month retainers |
| The residue is structural | `deslop` measured 2,912 reviewer-found defects in agent PRs; only 6 were classic slop patterns. The rest were stale references, duplicated logic, orphaned code |
| The metric is wrong | SD Times (Sept 2026): lines removed is a misleading cleanup metric; the real failures are duplicate logic, missing guards, accessibility, and coverage |

Caveat: GitClear and Sonar are vendor research. The direction is consistent
across sources, but the exact figures should be presented as directional, not
audited.

### 3.2 The detection layer is crowded

| Tool | What it owns | What it lacks |
|---|---|---|
| **Knip** | Entry/plugin breadth (~86M downloads/month, 21k+ public projects) | Verified deletion; fix is best-effort |
| **Fallow** | Rust speed, 100+ plugins, LSP/MCP/Node API (5.4k stars) | Verified deletion loop; audit scopes to changed files |
| **aislop** | "AI slop" keyword (~110k downloads/month), 50+ rules, hooks, CI, MCP, score | Baseline-backed deletion proof; safe mode refuses deletion |
| **Vercel `next-bundle-optimizer`** | Next.js bundle graph + AI optimization | Source-level deletion safety; project checks after deletion |
| **CodeTruth** | Agent-facing deletion-safety evidence (`safe_to_delete`, `likely_dead`, `uncertain`, `used`), MCP, baseline | A deletion PR with batches, bisection, and a receipt; JS/TS is a plugin, not the core |
| **Necro** | Confidence tiers, evidence chains, `test-only` verdict, MCP `verify_removal` in throwaway worktrees | A productized PR flow, build-graph evidence, a precision corpus |
| **fossil-mcp** | "Vibe coding" MCP, 16 languages, Rust | Stale; no verifier; shallow Next.js |
| **ts-unused / deadset-ts** | Type-member and prop depth | Graph-level reachability; verification |
| **honestcode** | Agent-facing verification of hallucinated symbols | Deletion workflow; project checks |

The conclusion is not "there is no competition." It is "the competition is at
detection or advisory safety; the verified PR is still open."

### 3.3 The cleanup market is bifurcated

| Buyer | What they want | What they pay for |
|---|---|---|
| Non-technical founder on Lovable/Replit/Bolt | Someone to fix the app | A **service**: $299–$40k, done for them |
| Professional team using agents | A workflow step | A **tool**: CI, PRs, proof |
| Agency doing cleanups | Faster delivery, client evidence | A **tool** that produces a verified report |

A CLI positioned at "vibe coders" speaks to the first group, who will not
install a Node package and will not pay for a dev tool. The second group is the
one with a build, CI, and budget — and they do not describe themselves as vibe
coders.

The agency is an attractive early segment because it already has the pain, the
budget, and a reason to show proof to a client. It is a beachhead, not the
identity.

---

## 4. The positioning decision

### 4.1 Category

**Verified cleanup for AI-assisted code.**

Not "vibe coding cleanup." Not "AI slop scanner." Not "dead-code analyzer."

### 4.2 Why "vibe coding" is a hook, not a home

- **It attracts the wrong buyer.** The phrase signals a non-technical founder
  who wants a service, not a team that wants a CI step.
- **It is becoming stigmatized.** Trust in AI-generated code is low; do not make
  the buyer's identity the product's identity.
- **It points at the wrong metric.** "Cleanup" suggests lines removed. The SD
  Times critique is correct: a smaller repo can still have duplicate
  data-fetching, missing authorization, and accessibility failures. Shrike should
  be measured by verified deletions, not deletion volume.
- **It is already occupied.** aislop owns the "AI slop" keyword with a
  multi-language, hook-first, score-driven product. fossil-mcp, vibeclean,
  deslop, slopscore, and a long tail of Claude Code skills fill the rest.
  Fighting for the phrase is a losing game.

The phrase is still useful at the top of the funnel. Use it in launch content,
SEO, and teardowns. Keep it out of the category, the H1, and the product name.

### 4.3 Message hierarchy

```text
H1:  Delete what your agent left behind. Prove nothing broke.

Sub: Shrike finds dead code across your whole Next.js repo — including
     the code your last PR orphaned — removes it in verified batches,
     and attaches the build/test receipt.

CTA: Open a verified cleanup PR.

Proof line: Built for the vibe-coding era.
            Trusted by teams that ship.
```

### 4.4 Say / don't say

| Say | Don't say |
|---|---|
| verified cleanup | slop score |
| proof-carrying deletion | vibe cleanliness |
| post-agent cleanup | AI slop detector |
| the cleanup pass | cleanup service |
| AI-assisted code | lines removed |
| verified deletion PR | one-click clean |
| orphan delta | unused code list |

### 4.5 One-sentence test

If a feature does not make this sentence more true, it is out of scope:

> **Shrike deletes the code that agents and PRs leave behind — and proves the
> app still builds.**

---

## 5. The wedge

### 5.1 Whole-graph PR orphan delta

A PR removes the only caller of a helper in a file it never touched. Every
changed-file audit reports nothing. Shrike compares the merge-base graph to the
head graph and reports the helper, its exclusive dependencies, and the PR that
orphaned them.

This is the signal code review cannot see and incumbents deliberately filter
out. Design: [04-whole-graph-delta.md](04-whole-graph-delta.md).

### 5.2 Verified deletion batches

Compute the cascade closure, condense cycles, partition into independent
batches, verify each against a baseline, bisect failures, and revert only the
culprit. Design: [05-verified-deletion.md](05-verified-deletion.md).

### 5.3 Portable receipt

Emit a content-addressed JSON proof for every verified batch: graph hashes,
witnesses, patches, checks, baseline status, build evidence, and outcome. The
receipt is the PR comment, the CI artifact, and the audit trail. Design:
[05-verified-deletion.md](05-verified-deletion.md) §6.

### 5.4 Build-graph evidence

Use `next analyze export` to confirm that a dead subtree is absent from client
bundles, attribute bytes, and catch dynamic edges the static graph missed.
Design: [06-build-graph-and-nextjs.md](06-build-graph-and-nextjs.md).

### 5.5 Rust speed and determinism

The engine parses and indexes 10,000 files in seconds with bounded memory and
produces byte-identical output for identical input. That makes the post-agent
hook and the CI receipt reproducible. Design:
[01-architecture.md](01-architecture.md) and [03-algorithms.md](03-algorithms.md).

---

## 6. Buyer

### 6.1 Primary

Tech lead or staff engineer at a 5–50 person team shipping a Next.js
application with an AI coding agent, with a real build and CI.

They care about:

- not breaking production;
- reviewing a diff they can trust;
- spending minutes, not a day, on cleanup;
- a signal in the PR, not another dashboard.

### 6.2 Secondary

An agency doing AI-code cleanup. They care about:

- finding the removable surface quickly;
- producing evidence for the client;
- not being blamed for a regression;
- repeatable delivery.

### 6.3 Not the buyer

The non-technical founder on a no-code platform. They are a content audience.
If they convert, they should convert into a service partner, not a self-serve
CLI user.

---

## 7. Why this is defensible

1. **The verifier is an operations product.** It mutates the repo, runs the
   project's checks, handles flaky tests, and reverts. Read-only and advisory
   incumbents have no incentive to become operations tools.
2. **The delta needs two graphs.** A file-scoped audit cannot produce a
   whole-graph orphan delta without changing its architecture.
3. **The build join needs Next.js depth.** A generic analyzer will not build a
   Turbopack module-to-file join.
4. **Receipts compound.** A rule can be copied in an afternoon. A precision
   corpus and a history of merged verified PRs cannot.
5. **The combination is the moat.** CodeTruth and Necro can copy a verdict
   format; they cannot copy the PR pipeline, the build join, the corpus, and the
   receipt history without becoming a different product.

---

## 8. Falsification tests

- If ten Next.js teams are offered a verified deletion PR versus a list of
  findings and prefer the list, the wedge is wrong.
- If Fallow, CodeTruth, or Necro ships baseline-aware verification with
  receipts, batch bisection, and a build-graph join, the window closes.
- If the orphan delta is mostly noise on real repositories (generated code,
  test-only entries, monorepo boundaries), fix the abstraction before scaling.
- If design partners merge the PR but do not run it again, the retention loop is
  wrong.
- If verification takes more than 15 minutes on a typical app, the tiered path
  becomes the product.
- If "vibe coding cleanup" outperforms "verified cleanup" in landing-page tests
  by a wide margin, use it in acquisition — but keep the category and the
  receipt.

---

## 9. Sources

- GitClear, *The Maintainability Gap* (August 2026) and *Write-Only Mode* (2026).
- Sonar, AI technical-debt survey, 2026.
- SD Times, "Vibe code cleanup is measuring the wrong thing" (September 2026).
- Fallow documentation: dead-code, auto-fix, audit.
- Knip documentation: Next.js plugin, FAQ, known issues, auto-fix.
- aislop README and docs, 2026.
- CodeTruth README, 2026.
- Necro README, 2026.
- fossil-mcp README, 2026.
- Vercel, "Ship less JavaScript in Next.js with your AI agent" (October 2026).
- Next.js docs: package bundling, `next analyze export`.
- Full citations and dates: [11-market-research.md](11-market-research.md) and
  [`../research/06-sources.md`](../research/06-sources.md).
