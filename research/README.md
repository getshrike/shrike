# Research

The evidence behind Shrike 1.0. This directory answers four questions:

1. What do competitors do, and what do they not do?
2. What technical substrate is available (parser, resolver, build graph, Rust)?
3. What does the market look like, and who is the buyer?
4. What prior art should inform the algorithm and the trust model?

Research is dated. A claim without a date and a source is not research.

---

## Index

| Document | Purpose | Confidence |
|---|---|---|
| [`00-methodology.md`](00-methodology.md) | Sources, dates, evidence standards | — |
| [`01-competitive-landscape.md`](01-competitive-landscape.md) | Knip, Fallow, aislop, CodeTruth, Necro, Vercel, others | medium–high |
| [`02-technical-substrate.md`](02-technical-substrate.md) | oxc, Rust crates, TypeScript 7, resolution, caching | medium |
| [`03-nextjs-build-graph.md`](03-nextjs-build-graph.md) | `next analyze export`, Turbopack, client-reference manifest | medium |
| [`04-prior-art-and-academia.md`](04-prior-art-and-academia.md) | Dead-code analysis, slicing, debloating, delta debugging | high |
| [`05-market-signals.md`](05-market-signals.md) | GitClear, Sonar, SD Times, pricing, demand | medium |
| [`06-sources.md`](06-sources.md) | Full source list with dates and URLs | — |

---

## How this research is used

| Research | Feeds |
|---|---|
| Competitive landscape | [`../docs/00-thesis-and-positioning.md`](../docs/00-thesis-and-positioning.md), [`../docs/11-market-research.md`](../docs/11-market-research.md) |
| Technical substrate | [`../docs/01-architecture.md`](../docs/01-architecture.md), [`../docs/03-algorithms.md`](../docs/03-algorithms.md) |
| Next.js build graph | [`../docs/06-build-graph-and-nextjs.md`](../docs/06-build-graph-and-nextjs.md) |
| Prior art | [`../docs/03-algorithms.md`](../docs/03-algorithms.md) |
| Market signals | [`../docs/11-market-research.md`](../docs/11-market-research.md) |

Research that does not inform a decision is not added. A note that only confirms
a belief is a diary entry, not evidence.

---

## Confidence levels

| Level | Meaning |
|---|---|
| `high` | Primary source, verified in code or documentation, current |
| `medium` | Primary source with a caveat, or a vendor claim with a plausible method |
| `low` | Secondary source, inference, or an unverified number |

Every document states its confidence. Every table row that matters carries a
source and a date.

---

## Update rules

- Add a dated entry; do not rewrite history.
- Update the source list in the same change.
- If a finding contradicts a design doc, open a decision in
  [`../docs/12-decision-log.md`](../docs/12-decision-log.md).
- If a competitor ships a wedge feature, record it and reassess
  [`../docs/10-non-goals-and-risks.md`](../docs/10-non-goals-and-risks.md).
- Remove a claim that cannot be reproduced; do not keep it for rhetoric.

---

## What this directory is not

- Not a marketing brief. Use [`../docs/11-market-research.md`](../docs/11-market-research.md)
  for positioning.
- Not a benchmark report. Raw results live in `../benchmarks/reports/`.
- Not a substitute for the design docs. It informs them; it does not override
  them.
