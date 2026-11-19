# 06 — Sources

> **Parent:** [README.md](README.md) · **Status:** source log · **Date:** 2026-11-20
> **Rule:** every external fact in this package is traceable to a row below. Observation
> dates are when the source was read, not when it was published.

---

## 1. Competitors

| ID | Source | Supports | Observed | Confidence |
|---|---|---|---|---|
| C-01 | Knip documentation, <https://knip.dev> | Entry/plugin model, CI guidance, fix behavior | 2026-11-20 | high |
| C-02 | Knip repository, <https://github.com/webpro-nl/knip> | Architecture, plugin count, license | 2026-11-20 | high |
| C-03 | Fallow repository and docs | Rust/Oxc engine, audit mode, LSP/MCP/API, benchmark claim | 2026-11-20 | medium |
| C-04 | aislop README and npm page, <https://www.npmjs.com/package/aislop> | Rules, hooks, score, safe mode | 2026-11-20 | medium |
| C-05 | CodeTruth repository and README | `safe_to_delete` labels, MCP, baseline, JS/TS plugin | 2026-11-20 | medium |
| C-06 | Necro repository and README | Confidence tiers, `verify_removal`, worktree | 2026-11-20 | medium |
| C-07 | Vercel, "Ship less JavaScript in Next.js with your AI agent" (Oct 2026) | Bundle optimizer positioning | 2026-11-20 | medium |
| C-08 | fossil-mcp repository | Multi-language MCP claim | 2026-11-20 | low–medium |
| C-09 | ts-unused / deadset-ts repositories | Type-member and prop analysis | 2026-11-20 | medium |
| C-10 | honestcode repository | Hallucinated-symbol verification | 2026-11-20 | low–medium |

**Numbers:** Knip ~86M downloads/month and 21k+ projects, Fallow 5.4k stars,
aislop ~110k downloads/month, CodeTruth 7 stars, Necro 0 stars. All observed
2026-11-20. These are attention signals, not revenue or retention.

---

## 2. Next.js and the build graph

| ID | Source | Supports | Observed | Confidence |
|---|---|---|---|---|
| N-01 | Next.js docs, file conventions, <https://nextjs.org/docs/app/api-reference/file-conventions> | App Router entries | 2026-11-20 | high |
| N-02 | Next.js docs, `page`, `layout`, `route`, `proxy`, `instrumentation` | Entry semantics, exports | 2026-11-20 | high |
| N-03 | Next.js docs, metadata files (`sitemap`, `robots`, `manifest`, icons, OG images) | Metadata entries | 2026-11-20 | high |
| N-04 | Next.js docs, `use server` and `use client` directives | Server actions, client boundaries | 2026-11-20 | high |
| N-05 | Next.js docs, `use cache` | Next.js 16 cache directives | 2026-11-20 | medium |
| N-06 | Next.js docs, Pages Router routing and data fetching | Pages Router entries | 2026-11-20 | high |
| N-07 | Next.js 16.4 release notes | `next analyze export`, Turbopack graph | 2026-11-20 | medium |
| N-08 | Next.js docs, `next analyze export` | JSONL module/chunk graph | 2026-11-20 | medium |
| N-09 | Next.js docs, client-reference manifest | Server action and client boundary mapping | 2026-11-20 | medium |

**Caveat:** the `next analyze export` schema is young. Treat it as a versioned
external contract, not a stable internal API.

---

## 3. Parser and substrate

| ID | Source | Supports | Observed | Confidence |
|---|---|---|---|---|
| T-01 | `oxc` repository, <https://github.com/oxc-project/oxc> | Parser, semantic, resolver | 2026-11-20 | high |
| T-02 | `oxc_parser` docs | AST, spans, diagnostics | 2026-11-20 | high |
| T-03 | `oxc_semantic` docs | Scopes, symbols, references, unresolved | 2026-11-20 | high |
| T-04 | `oxc_resolver` docs | tsconfig paths, exports, workspace | 2026-11-20 | high |
| T-05 | TypeScript 7 release notes and semantic API discussions | stdio semantic server, not in-process | 2026-11-20 | medium |
| T-06 | Rust crate docs: `rayon`, `serde`, `blake3`, `rustc-hash`, `thiserror`, `tracing`, `insta`, `proptest`, `criterion` | Engine dependencies | 2026-11-20 | high |
| T-07 | `petgraph` docs | Optional graph utilities and test oracle | 2026-11-20 | high |

---

## 4. Market signals

| ID | Source | Supports | Observed | Confidence |
|---|---|---|---|---|
| M-01 | GitClear, *The Maintainability Gap* (Aug 2026) | Duplication and churn trends | 2026-11-20 | medium |
| M-02 | GitClear, *Write-Only Mode* (2026) | AI code volume and churn | 2026-11-20 | medium |
| M-03 | Sonar, AI technical-debt survey (2026) | 53% negative debt impact | 2026-11-20 | medium |
| M-04 | SD Times, "Vibe code cleanup is measuring the wrong thing" (Sept 2026) | Lines-removed critique | 2026-11-20 | medium |
| M-05 | Upwork and agency pricing observations (2026) | $299–$40k cleanup range | 2026-11-20 | low–medium |
| M-06 | `deslop` analysis (2026) | 2,912 reviewer-found defects; 6 classic slop | 2026-11-20 | low–medium |

**Caveat:** M-01 to M-03 are vendor research. Use directionally, not as audited
facts. M-05 is an observation, not a survey.

---

## 5. Prior art

| ID | Source | Supports | Confidence |
|---|---|---|---|
| P-01 | M. Weiser, "Program Slicing," *IEEE TSE*, 1984 | Static slicing, dependence | high |
| P-02 | F. Tip, "A Survey of Program Slicing Techniques," 1995 | Slicing taxonomy | high |
| P-03 | A. Zeller and R. Hildebrandt, "Simplifying and Isolating Failure-Inducing Input," *IEEE TSE*, 2002 | Delta debugging | high |
| P-04 | A. Zeller, *Why Programs Fail* (2nd ed.) | Debugging and minimization | high |
| P-05 | Debloating literature (e.g., Quach et al., "Debloating Software through Piece-Wise Compilation and Loading") | Workload-defined liveness | medium |
| P-06 | Change-impact analysis surveys | Reachability diff, impact | medium |

The academic references inform the algorithm; they are not product dependencies.

---

## 6. Local observations

| ID | Observation | Date |
|---|---|---|
| L-01 | Working directory is `/home/mark/code/projects/shrike`; the repository is not a git repo | 2026-11-20 |
| L-02 | The prior tree was backed up outside the repository and removed; the new package is greenfield | 2026-11-20 |
| L-03 | The design package contains 13 docs, 7 plans, 9 prompts, 8 research docs, and 6 schemas | 2026-11-20 |

---

## 7. How to update this log

1. Add a row with a stable ID.
2. Record the observation date and the confidence.
3. Link the claim it supports.
4. If a source is superseded, mark it `superseded`; do not delete it.
5. If a number cannot be reproduced, downgrade its confidence or remove the
   claim that depends on it.

A source list that grows without dates is a bibliography, not evidence.
