# 11 — Market Research

> **Parent:** [MASTER_PROMPT.md](../MASTER_PROMPT.md) · **Tracker:** [TRACKER.md](../TRACKER.md)
> **Status:** research digest · Authored 2026-11-20
> **Method:** repository review, competitor documentation, primary sources, npm
> and GitHub metadata. Dates are observation dates unless stated.

---

## 1. Why this document exists

The product direction is a bet on a gap. This document records the evidence for
that bet, the competitors that could close it, and the numbers that would change
our mind. It is the source for the claims in
[00-thesis-and-positioning.md](00-thesis-and-positioning.md).

---

## 2. Demand signals

### 2.1 Code quality

| Signal | Number | Source |
|---|---|---|
| Block duplication, 2023 | 40.3 | GitClear, *The Maintainability Gap* (Aug 2026) |
| Block duplication, YTD 2026 | 73.0 | Same; +81%, highest on record |
| Two-week churn, 2023–2024 | ~16% | Same |
| Two-week churn, 2025–2026 | ~19% | Same |
| AI power-user churn | 9× | GitClear cohort analysis |
| Developers citing negative debt impact from AI code that looked correct | 53% | Sonar, 2026 survey |
| Reviewer-found defects in agent PRs | 2,912 | `deslop` analysis, 2026 |
| Of those, classic "slop" patterns | 6 | Same |
| Lines removed as a quality metric | misleading | SD Times, Sept 2026 |

**Interpretation:** the pain is real and the residue is structural (stale
references, orphaned code, duplicated logic), not cosmetic. A tool that removes
dead code and proves the build still works addresses the defect class that
reviewers actually find.

**Caveat:** GitClear and Sonar are vendor research. Treat the direction as
consistent and the exact figures as directional.

### 2.2 Cleanup spend

| Segment | Price | Source |
|---|---|---|
| Low-end freelance cleanup | $299 | Upwork listings, 2026 |
| Average completed invoice | ~$7,200 | Marketplace analysis, 2026 |
| Audit | $2k–$10k | Agency pricing, 2026 |
| Retainer | $500–$2k/month | Agency pricing, 2026 |
| High-end project | up to $40k | Agency pricing, 2026 |

The buyer with a build and CI will pay for a tool that reduces the cleanup to a
reviewable PR. The non-technical founder buys a service.

### 2.3 Agent adoption

Claude Code, Cursor, Codex, and OpenCode are normal workflow in 2026. Hooks are
first-class in Claude Code and Cursor. The cleanup moment is after the agent
finishes, which is why the hook is the first distribution surface.

---

## 3. Competitor matrix

### 3.1 Knip

| Attribute | Value |
|---|---|
| Category | Dead-code and unused-dependency finder |
| Language | TypeScript/JavaScript |
| Distribution | ~86M npm downloads/month; 21k+ public projects (project site, Nov 2026) |
| Strengths | Entry/plugin breadth, configuration, mature rule set, CI guide |
| Weaknesses | Read-only at the trust layer; fix is best-effort; changed-file CI guidance |
| Threat | High distribution; could add verification |

Knip owns the "find unused things" category. It does not run the project's
checks after a deletion, bisect failures, or produce a receipt.

### 3.2 Fallow

| Attribute | Value |
|---|---|
| Category | Dead-code analysis, Rust + Oxc |
| Distribution | 5.4k GitHub stars |
| Performance claim | 20,558 files in 2.95 s |
| Strengths | Speed, 100+ framework plugins, changed-file audit, LSP, MCP, Node API, auto-fix |
| Weaknesses | No baseline-backed deletion loop; audit scopes to changed files; fix tells the user to run build/tests |
| Threat | High technical overlap; closest on speed and framework breadth |

Fallow is the closest on the analysis layer. It is not an operations product: it
does not apply, verify against a baseline, bisect, revert, and receipt.

### 3.3 aislop

| Attribute | Value |
|---|---|
| Category | "AI slop" detection and cleanup |
| Distribution | ~110k npm downloads/month |
| Strengths | 50+ rules, multi-language, hooks for Claude/Cursor/Gemini, CI, MCP, score, fix |
| Weaknesses | Score is uncalibrated; safe mode refuses code deletion; no baseline-backed deletion proof |
| Threat | Owns the "AI slop" keyword and the hook distribution |

aislop proves the hook distribution works. It also proves the score is a
commodity. Shrike should copy the distribution and reject the score.

### 3.4 CodeTruth

| Attribute | Value |
|---|---|
| Category | Agent-facing deletion-safety evidence |
| Distribution | 7 GitHub stars (Nov 2026) |
| Strengths | `safe_to_delete` / `likely_dead` / `uncertain` / `used`; MCP; baseline; cross-repo and runtime evidence; claims 0 false positives on 36,457 symbols |
| Weaknesses | Python-first with a JS/TS plugin; advisory verdict; no deletion PR, batches, bisection, or receipt |
| Threat | **Direct adjacent competitor.** Legitimizes "safe deletion" as a category |

CodeTruth is the reason "safe deletion" is no longer a unique wedge. The wedge
must be the verified PR and the receipt, not the verdict.

### 3.5 Necro

| Attribute | Value |
|---|---|
| Category | Dead-code analysis with removal verification |
| Distribution | 0 GitHub stars (Nov 2026) |
| Strengths | Confidence tiers, evidence chains, `test-only` verdict, MCP `verify_removal` in throwaway worktrees |
| Weaknesses | Early; no productized PR flow, build-graph evidence, or precision corpus |
| Threat | **Direct adjacent competitor.** Validates worktree verification |

Necro's `verify_removal` is the closest feature to Shrike's verifier. Shrike's
difference is the whole-graph delta, independent batches, bisection, a receipt,
and a PR.

### 3.6 Vercel

| Attribute | Value |
|---|---|
| Category | Next.js platform and bundle optimization |
| Strengths | Owns Next.js, Turbopack, `next analyze export`, and a bundle optimizer with an agent |
| Weaknesses | Optimizes what ships, not source-level deletion safety; no project checks after deletion |
| Threat | Could add dead-code cleanup to the platform |

Do not compete on bundle size. Consume the build graph as evidence.

### 3.7 Others

| Tool | Claim | Gap |
|---|---|---|
| fossil-mcp | "Vibe coding" MCP, 16 languages, Rust | Stale; no verifier; shallow Next.js |
| ts-unused / deadset-ts | Type-member and prop depth | No graph-level reachability or verification |
| honestcode | Agent-facing verification of hallucinated symbols | No deletion workflow |
| deslop | Reviewer-found defect analysis | Research, not a product |
| vibeclean, slopscore | Keyword tools | No verification |

The long tail confirms the keyword is crowded and the verification layer is
not.

---

## 4. What the competitors have in common

1. **Detection is the product.** The output is a report, a score, or a verdict.
2. **The project's checks are the user's problem.** At most, the tool tells the
   user to run build and tests.
3. **Changed-file scoping.** CI guidance narrows findings to touched files to
   control noise, which structurally hides orphans in untouched files.
4. **No portable proof.** A JSON report is not a content-addressed receipt tied
   to patches and checks.
5. **No batch isolation.** Fixes are applied wholesale or one at a time; there is
   no independent batch with bisection and partial acceptance.

These are architectural choices, not missing features. Copying Shrike's output
requires changing the architecture.

---

## 5. The opening

### 5.1 The sentence

> **Shrike finds what this PR orphaned, deletes it in verified batches, and gives
> you a receipt that proves the app still builds.**

### 5.2 Why now

- Agents are the primary author, so residue is continuous.
- The Next.js build graph is observable.
- Incumbents stayed read-only or advisory.
- Rust makes the two-snapshot workload fast enough for a hook.

### 5.3 Why it is defensible

- The verifier is an operations product.
- The delta needs two snapshots and stable IDs.
- The build join needs Next.js depth.
- Receipts and a precision corpus compound.
- The combination is hard to copy without becoming a different product.

### 5.4 Why it could fail

- The delta is noisy on real PRs.
- Verification is too slow.
- A well-funded incumbent ships the same loop.
- The buyer prefers a report or a service.
- Next.js changes the build graph contract.

The falsification tests in [10-non-goals-and-risks.md](10-non-goals-and-risks.md)
§3 are the early warning system.

---

## 6. Pricing and packaging (direction, not commitment)

| Tier | Price | Contents |
|---|---|---|
| Free | $0 | Local CLI, scan, delta, proof, hook, one receipt |
| Pro | $19–$29/dev/month | CI Action, PR flow, build evidence, receipt history |
| Team | $49–$99/dev/month | Policy, multiple repos, audit export |
| Agency | Custom | Multi-repo cleanup, white-label receipt |

v1 is free for design partners. Pricing is decided after the loop is proven
(Q-028).

Open-source libraries and small teams get the free tier. The paid tier is for
teams that run it in CI on every PR.

---

## 7. Distribution plan

1. **Hook first.** Install in Claude Code, Cursor, and OpenCode.
2. **PR second.** A verified deletion PR is the demo.
3. **CI third.** The read-only delta gate for cautious teams.
4. **Content.** Ten public Next.js teardowns with before/after receipts.
5. **Design partners.** Three teams, read-only first, then prune.

The landing page tests two positionings:

- A: "Delete what your agent left behind. Prove nothing broke."
- B: "The cleanup pass for AI-generated code."

Measure qualified sign-ups (a repository connected or a CLI run), not page views.

---

## 8. Sources

- Knip documentation and project site, November 2026.
- Fallow README and benchmarks, November 2026.
- aislop README and npm metadata, November 2026.
- CodeTruth README, November 2026.
- Necro README, November 2026.
- Vercel, "Ship less JavaScript in Next.js with your AI agent," October 2026.
- Next.js documentation, `next analyze export`, 16.4.
- GitClear, *The Maintainability Gap*, August 2026.
- Sonar, AI technical-debt survey, 2026.
- SD Times, "Vibe code cleanup is measuring the wrong thing," September 2026.
- Upwork and agency pricing observations, 2026.

Full citations and URLs: [`../research/06-sources.md`](../research/06-sources.md).

---

## 9. Research gaps

- No public, audited precision number from any competitor.
- No public data on how often a PR orphans code in an untouched file.
- No public measurement of verification cost after a dead-code fix.
- No public data on whether teams prefer a report or a PR.

These gaps are the design-partner research agenda. They are also the numbers
that would justify or kill the product.
