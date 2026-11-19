# 01 — Competitive Landscape

> **Parent:** [README.md](README.md) · **Status:** research note · **Date:** 2026-11-20 · **Confidence:** medium–high
> **Method:** repository review, official docs, npm and GitHub metadata, release notes.

---

## 1. Question

What do the strongest dead-code and cleanup tools do, what do they deliberately
not do, and where is the remaining gap for Shrike 1.0?

---

## 2. Summary

The market is crowded at **detection** and **advisory safety**. It is thin at
**verified deletion**. The closest adjacent competitors are CodeTruth and Necro
(safety evidence for agents), Fallow (fast analysis with an audit mode), and
Knip (distribution and configuration breadth). None of them runs the project's
checks against a baseline, bisects a failed batch, reverts only the culprit, and
emits a portable receipt as a PR.

The gap is not "find dead code." It is:

> **Find what this PR orphaned anywhere in the repo, delete it in verified
> batches, and prove the app still builds.**

---

## 3. Knip

| Attribute | Observation |
|---|---|
| Language | TypeScript |
| Category | Unused files, exports, dependencies, types |
| Distribution | ~86M npm downloads/month; 21k+ public projects (project site, Nov 2026) |
| Strengths | Entry/plugin breadth, configuration, CI guidance, mature rules |
| Weaknesses | Read-only at the trust layer; fix is best-effort; changed-file CI scoping |
| Evidence | Official docs and repository, Nov 2026 |

**What it owns:** the "find unused things" category and the plugin ecosystem.

**What it does not do:** apply a deletion, run typecheck/lint/test/build after it,
compare against a baseline, bisect a failure, revert a culprit, or emit a
content-addressed receipt.

**What Shrike should learn:** configuration clarity and entry breadth at the
framework level; distribution through docs and CI.

**What Shrike should not copy:** a 100+ plugin catalogue.

---

## 4. Fallow

| Attribute | Observation |
|---|---|
| Language | Rust + Oxc |
| Category | Dead-code analysis, changed-file audit, auto-fix |
| Distribution | 5.4k GitHub stars (Nov 2026) |
| Performance claim | 20,558 files in 2.95 s (vendor benchmark) |
| Strengths | Speed, framework plugins, LSP, MCP, Node API, audit mode |
| Weaknesses | No baseline-backed deletion loop; audit scopes to changed files; fix tells the user to run build/tests |
| Evidence | README and docs, Nov 2026 |

**What it owns:** the fast-analysis layer and developer surface (LSP, MCP, API).

**What it does not do:** prove the deletion against the project's own checks,
isolate independent batches, bisect, or receipt.

**Threat:** highest technical overlap. If Fallow adds verification and receipts,
the window narrows. Shrike's defense is the orphan delta, the build join, the
precision corpus, and the operations depth.

**What Shrike should learn:** Rust + Oxc is the right substrate; speed is a real
differentiator; developer surfaces matter.

---

## 5. aislop

| Attribute | Observation |
|---|---|
| Language | Multi-language |
| Category | "AI slop" detection and cleanup |
| Distribution | ~110k npm downloads/month (Nov 2026) |
| Strengths | 50+ rules, hooks for Claude/Cursor/Gemini, CI, MCP, score, fix |
| Weaknesses | Score is uncalibrated; safe mode refuses code deletion; no baseline-backed deletion proof |
| Evidence | README and docs, Nov 2026 |

**What it owns:** the "AI slop" keyword and the hook distribution.

**What it does not do:** delete code with a verified build. Its safe mode
explicitly refuses the risky part.

**What Shrike should learn:** hooks work; a one-screen result is the right
format; CI and MCP are expected surfaces.

**What Shrike should not copy:** the score. A score is uncalibrated, gameable,
and the wrong metric.

---

## 6. CodeTruth

| Attribute | Observation |
|---|---|
| Language | Python-first, JS/TS plugin |
| Category | Agent-facing deletion-safety evidence |
| Distribution | 7 GitHub stars (Nov 2026) |
| Strengths | `safe_to_delete` / `likely_dead` / `uncertain` / `used`; MCP; baseline; cross-repo and runtime evidence; claims 0 false positives on 36,457 symbols |
| Weaknesses | Advisory verdict; no deletion PR, batches, bisection, or receipt; JS/TS is a plugin |
| Evidence | README, Nov 2026 |

**What it owns:** the vocabulary of deletion safety for agents.

**What it does not do:** operate on the repository with a verified PR.

**Threat:** direct adjacent competitor. It legitimizes the category and could add
a PR flow.

**What Shrike should learn:** evidence labels and an MCP surface are useful; a
claim of zero false positives needs a published method.

**What Shrike should not copy:** a Python-first core with a shallow JS/TS plugin.

---

## 7. Necro

| Attribute | Observation |
|---|---|
| Language | TypeScript/Python |
| Category | Dead-code analysis with removal verification |
| Distribution | 0 GitHub stars (Nov 2026) |
| Strengths | Confidence tiers, evidence chains, `test-only` verdict, MCP `verify_removal` in throwaway worktrees |
| Weaknesses | Early; no productized PR flow, build-graph evidence, or precision corpus |
| Evidence | README, Nov 2026 |

**What it owns:** the idea of verifying a removal in a worktree.

**What it does not do:** batch, bisect, receipt, or open a PR.

**Threat:** direct adjacent competitor on the verification idea. Shrike's
difference is the whole-graph delta, independent batches, a receipt, and the PR.

---

## 8. Vercel

| Attribute | Observation |
|---|---|
| Category | Next.js platform, Turbopack, bundle optimization |
| Strengths | Owns Next.js, `next analyze export`, and a bundle optimizer with an agent |
| Weaknesses | Optimizes what ships, not source-level deletion safety; no project checks after deletion |
| Evidence | Next.js docs and Vercel posts, Oct–Nov 2026 |

**What it owns:** the build graph and the bundle-optimization narrative.

**What Shrike should do:** consume the build graph as evidence, never compete on
bundle optimization.

---

## 9. Long tail

| Tool | Claim | Gap |
|---|---|---|
| fossil-mcp | "Vibe coding" MCP, 16 languages, Rust | Stale; no verifier; shallow Next.js |
| ts-unused / deadset-ts | Type-member and prop depth | No graph-level reachability or verification |
| honestcode | Agent-facing verification of hallucinated symbols | No deletion workflow |
| deslop | Reviewer-found defect analysis | Research, not a product |
| vibeclean, slopscore | Keyword tools | No verification |

The long tail confirms the keyword is crowded and the verification layer is not.

---

## 10. Capability matrix

| Capability | Knip | Fallow | aislop | CodeTruth | Necro | Shrike |
|---|---|---|---|---|---|---|
| Dead-code detection | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Framework breadth | ✓✓ | ✓✓ | ✓ | plugin | limited | Next/React deep |
| Whole-graph PR orphan delta | — | changed files | — | — | — | ✓ |
| Witness / proof trace | partial | partial | — | evidence | evidence | ✓ |
| Apply deletion | fix | fix | safe subset | — | worktree | ✓ |
| Baseline comparison | — | — | — | baseline | — | ✓ |
| Independent batches | — | — | — | — | — | ✓ |
| Bisection | — | — | — | — | — | ✓ |
| Revert culprit only | — | — | — | — | — | ✓ |
| Portable receipt | — | — | — | — | — | ✓ |
| Build-graph evidence | — | — | — | — | — | ✓ |
| Speed (Rust) | — | ✓ | — | — | — | ✓ |
| Deterministic output | — | — | — | — | — | ✓ |

`✓✓` = category strength; `✓` = present; `—` = absent or not the product.

---

## 11. Conclusion

1. Detection is commoditized. Do not compete on rule count.
2. Advisory safety is now occupied. Do not compete on a verdict label.
3. The verified deletion PR with a receipt is still open.
4. The orphan delta is the sharpest signal because changed-file audits cannot
   produce it.
5. The build join is a defensible Next.js-specific advantage.
6. Rust speed and determinism make the hook and CI path viable.

The wedge is the combination, not any single row of the matrix.

---

## 12. What would change this conclusion

- A competitor ships verified deletion with receipts and a build join.
- The orphan delta proves noisy on real PRs.
- Verification cannot fit a usable CI budget.
- The buyer prefers a report or a service over a PR.

Track these in [`../docs/10-non-goals-and-risks.md`](../docs/10-non-goals-and-risks.md) §3.

---

## 13. Sources

See [`06-sources.md`](06-sources.md) for URLs, dates, and access dates.
