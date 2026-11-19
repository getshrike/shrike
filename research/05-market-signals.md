# 05 — Market Signals

> **Parent:** [README.md](README.md) · **Status:** research note · **Date:** 2026-11-20 · **Confidence:** medium
> **Method:** vendor research, industry press, marketplace pricing, competitor distribution.

---

## 1. Question

Is there a durable market for verified cleanup, and which buyer will pay for it?

---

## 2. Demand signals

### 2.1 Code quality

| Signal | Number | Source | Confidence |
|---|---|---|---|
| Block duplication, 2023 | 40.3 | GitClear, *The Maintainability Gap* (Aug 2026) | medium |
| Block duplication, YTD 2026 | 73.0 | Same | medium |
| Change | +81%, highest on record | Same | medium |
| Two-week churn, 2023–2024 | ~16% | Same | medium |
| Two-week churn, 2025–2026 | ~19% | Same | medium |
| AI power-user churn | 9× | GitClear cohort analysis | medium |
| Negative debt impact from AI code that looked correct | 53% | Sonar, 2026 | medium |
| Reviewer-found defects in agent PRs | 2,912 | `deslop`, 2026 | low–medium |
| Classic slop patterns among them | 6 | Same | low–medium |
| Lines removed as a quality metric | misleading | SD Times, Sept 2026 | medium |

**Interpretation:** the residue is structural. The defect class is stale
references, orphaned code, and duplicated logic — exactly what a graph-based
cleanup addresses.

**Caveat:** GitClear and Sonar are vendor research. Treat the direction as
consistent, the exact figures as directional.

### 2.2 Cleanup spend

| Segment | Price | Source | Confidence |
|---|---|---|---|
| Low-end freelance | $299 | Upwork, 2026 | medium |
| Average completed invoice | ~$7,200 | Marketplace analysis | low–medium |
| Audit | $2k–$10k | Agency pricing | medium |
| Retainer | $500–$2k/month | Agency pricing | medium |
| High-end project | up to $40k | Agency pricing | low |

The professional team buys a tool; the non-technical founder buys a service.

### 2.3 Agent adoption

- Claude Code, Cursor, Codex, and OpenCode are normal workflow.
- Hooks are first-class in Claude Code and Cursor.
- The cleanup moment is immediately after the agent finishes.

### 2.4 Competitor distribution

| Tool | Signal | Date | Confidence |
|---|---|---|---|
| Knip | ~86M npm downloads/month; 21k+ projects | Nov 2026 | medium |
| Fallow | 5.4k GitHub stars | Nov 2026 | medium |
| aislop | ~110k npm downloads/month | Nov 2026 | medium |
| CodeTruth | 7 GitHub stars | Nov 2026 | medium |
| Necro | 0 GitHub stars | Nov 2026 | medium |

Downloads and stars measure attention, not revenue or retention. They show that
the detection category has an audience; they do not prove that the verified
deletion category does.

---

## 3. Buyer

### 3.1 Primary

Tech lead or staff engineer at a 5–50 person team shipping a Next.js app with an
AI coding agent, a real build, and CI.

They care about:

- not breaking production;
- a reviewable diff;
- minutes, not a day;
- a PR signal, not a dashboard.

### 3.2 Secondary

An agency doing AI-code cleanup. They care about speed, client evidence, blame
avoidance, and repeatable delivery. A beachhead, not the identity.

### 3.3 Not the buyer

The non-technical founder on a no-code platform. They buy a done-for-you service,
not a CLI. They are a content audience.

---

## 4. Willingness to pay

| Segment | What they pay for | Model |
|---|---|---|
| Professional team | CI step, verified PRs, receipt | Per-seat SaaS or team license |
| Agency | Multi-repo cleanup, white-label proof | Custom |
| Open-source maintainer | Local scan, proof | Free |
| Non-technical founder | Done-for-you cleanup | Service partner |

v1 is free for design partners. Pricing is decided after the loop is proven
(Q-028).

---

## 5. Channels

| Channel | Fit | Why |
|---|---|---|
| Post-agent hook | High | Meets the developer in the loop |
| GitHub PR | High | The unit of review |
| CI Action | High | The gate |
| MCP | Medium | Agents ask for it; not the product |
| Content/SEO | Medium | "AI slop" is crowded; "verified cleanup" is not |
| Open-source | Medium | Trust and distribution |
| Paid ads | Low | Expensive; the buyer is narrow |

---

## 6. Positioning tests

### Test A

> Delete what your agent left behind. Prove nothing broke.

### Test B

> The cleanup pass for AI-generated code.

Measure qualified sign-ups (CLI install or repository connection), not page views.
Run for at least two weeks or 1,000 qualified visitors.

**Hypothesis:** A wins with the professional buyer; B wins with the content
audience. If B wins on qualified sign-ups, reconsider the category, not the
receipt.

---

## 7. Risks to the market thesis

| Risk | Signal | Response |
|---|---|---|
| Agents get better at subtraction | Residue declines | Pivot to verification and build attribution |
| Incumbent ships the loop | Announcement | Go deeper on orphan delta and corpus |
| Verification is too slow | Trial drop-off | Tiered checks; affected-only |
| Buyer prefers a service | Low CLI activation | Agency beachhead |
| "AI slop" fatigue | Ad costs rise | Lead with verified cleanup |
| Next.js changes the graph | Evidence gap | Static fallback |

---

## 8. What would change our mind

- If 10 teams prefer a report to a verified PR, the wedge is wrong.
- If fewer than 20% of real PRs have a genuine orphan after triage, the delta is
  too noisy.
- If verification exceeds 15 minutes on a typical app, the product is the tiered
  path.
- If design partners do not run it twice, retention is the problem.
- If an incumbent makes verified deletion free and reliable, move to the
  build-attribution niche or the agency workflow.

---

## 9. Sources

See [`06-sources.md`](06-sources.md).
