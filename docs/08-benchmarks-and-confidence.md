# 08 — Benchmarks and Confidence

> **Parent:** [MASTER_PROMPT.md](../MASTER_PROMPT.md) · **Tracker:** [TRACKER.md](../TRACKER.md)
> **Status:** canonical · Authored 2026-11-20

---

## 1. Confidence is table stakes; calibration is the moat

Every analyzer claims confidence. Knip has hints, Fallow has confidence levels,
`aislop` has a score, and every agent claims to know when it is unsure. A score
that is not calibrated against a labeled corpus is decoration.

Shrike's claim is narrower and stronger:

> For every rule and every evidence combination, we publish how often the
> finding was correct on a corpus of repositories where we know the answer.

That claim cannot be copied without doing the same work, and it is the claim a
skeptical staff engineer will ask for.

This document defines the corpus, the benchmarks, the measurement method, and
the reporting format. Actuals live in [`TRACKER.md`](../TRACKER.md) §5.

---

## 2. The must-not-report corpus

### 2.1 Purpose

The corpus is a set of patterns that **must never be reported as dead**. It is
the first gate, not the last. A single false positive on a route, a server
action, or a dynamic import is a release blocker.

The corpus is built from framework documentation and real applications, not
from the analyzer's output. Building it after the rules would encode the rules'
bugs.

### 2.2 Composition

| Category | Examples | Minimum |
|---|---|---|
| Next.js App Router conventions | `page`, `layout`, `loading`, `error`, `not-found`, `template`, `route` | 20 |
| Metadata files | `sitemap`, `robots`, `manifest`, `icon`, `opengraph-image` | 10 |
| Server actions | `"use server"` exports, form actions, bound actions | 10 |
| Client boundaries | `"use client"` modules, client components in server trees | 10 |
| Proxy and middleware | `proxy.ts`, `middleware.ts`, matchers | 5 |
| Dynamic imports | literal, template, variable, conditional | 15 |
| Re-exports and barrels | `export *`, aliased re-exports, namespace re-exports | 10 |
| Package public API | `package.json` exports, declaration files, entry points | 10 |
| Tests and configs | test setup, fixtures, config files | 10 |
| Framework data functions | `generateStaticParams`, `generateMetadata`, loaders | 10 |
| React roots and lazy | `createRoot`, `hydrateRoot`, `lazy`, route configs | 10 |
| Total | | **≥ 120** |

Phase 6 expands this to ≥ 250 entries.

### 2.3 Corpus entry format

```text
fixtures/must-not-report/<category>/<name>/
├── README.md          # what this proves, and the framework citation
├── repo/              # minimal project or file set
├── config.json        # shrike config for the fixture
└── expected.json      # { "findings": [] } or blocked findings with reasons
```

A corpus entry fails if Shrike reports **any** deletion candidate in `repo/`
that is not explicitly listed as blocked. Blocked findings (public API, unknown
edge) are allowed when the reason is correct; they are not "reported as dead."

### 2.4 Corpus gate

- Every PR runs the corpus.
- A new false positive fails CI.
- A corpus change requires a review and a framework citation.
- The corpus is versioned with the analyzer; a rule cannot ship without a green
  corpus.

---

## 3. The labeled set

The corpus proves the absence of known false positives. The labeled set measures
precision and recall on real repositories.

### 3.1 Selection

- 10–20 public TypeScript/JavaScript repositories.
- Mix: Next.js App Router, Pages Router, React SPA, component library, monorepo.
- Size: 200–10,000 files.
- Each repository pinned by commit.
- Licenses permit analysis.

### 3.2 Labeling

Two independent labelers review a stratified sample of candidate findings:

- **confirmed dead**: no reference, no entry, no dynamic path; removal is safe;
- **false positive**: a reference, entry, or dynamic path exists;
- **uncertain**: evidence is insufficient; excluded from precision.

Labeling rules:

- the labeler sees the witness and the proof trace;
- the labeler does not see the tool's confidence label;
- disagreements are adjudicated by a third reviewer;
- a 10% sample is re-labeled after two weeks to measure drift.

### 3.3 Metrics

```text
precision = confirmed dead / (confirmed dead + false positives)
recall    = confirmed dead / (confirmed dead + missed dead)
```

The labeled set is too small for recall to be stable; precision is the primary
metric. Report both with confidence intervals.

### 3.4 Per-rule calibration

For each rule and evidence combination:

| Rule | Certainty | Findings | Confirmed | False | Precision |
|---|---|---|---|---|---|
| unused-export | certain | 412 | 401 | 11 | 0.973 |
| unused-file | certain | 38 | 38 | 0 | 1.000 |
| unused-dependency | probable | 61 | 44 | 17 | 0.721 |

Rules below 0.95 precision are not enabled by default; they are reported as
`experimental` and blocked from auto-deletion.

This is the calibration table the product publishes. It is the reason a buyer
can trust the tool on their repository.

---

## 4. Performance benchmarks

### 4.1 Reference hardware

Record for every report:

- CPU model and core count;
- RAM;
- OS and kernel;
- filesystem (local SSD vs network);
- Rust version and build profile;
- Node version;
- Shrike version and commit.

### 4.2 Corpora

| Corpus | Files | Shape |
|---|---|---|
| `next-app-1k` | ~1,000 | Next.js App Router, realistic imports |
| `next-app-3k` | ~3,000 | App Router + component library |
| `monorepo-10k` | ~10,000 | pnpm workspace, 6 packages |
| `library-5k` | ~5,000 | React component library, public API heavy |

The corpora are generated from pinned public repositories, not synthetic
single-file loops. A synthetic corpus can hide resolver and barrel costs.

### 4.3 Method

- 3 warmups, 5 measured runs.
- Report median, p95, and peak RSS.
- Cold: cache removed before each run.
- Warm: cache present and valid.
- No other workload on the machine.
- Pin CPU frequency if possible; record if not.
- Run the engine directly, not through a shell wrapper.
- Measure wall time and peak RSS (`/usr/bin/time -v` or the harness).

### 4.4 Budgets

| Workload | Target |
|---|---|
| 1,000 files, cold scan | ≤ 1.5 s |
| 1,000 files, warm scan | ≤ 400 ms |
| 10,000 files, cold scan | ≤ 6 s |
| 10,000 files, warm scan | ≤ 2 s |
| 10,000 files, peak RSS | ≤ 600 MB |
| Delta on 1,000 files, warm | ≤ 1 s |
| Fast receipt | ≤ 60 s |
| Full receipt | ≤ 10 min |

The budgets are targets for the Rust engine. A missed budget is recorded with
the measurement and the optimization attempt before the target changes.

### 4.5 Regression policy

- `nightly.yml` runs on `main`, not on every PR.
- A > 20% regression on a budgeted workload fails the job.
- A regression is investigated or the budget is amended with a reason.
- Benchmark results are uploaded as a raw JSON artifact and recorded in
  [`../TRACKER.md`](../TRACKER.md) §5.

### 4.6 What not to benchmark

- microbenchmarks as product claims;
- "files per second" without file size and shape;
- a warm run presented as cold;
- a scan without resolution or entry detection;
- a benchmark on a machine that cannot be reproduced.

---

## 5. Product metrics

| Metric | Definition | Target |
|---|---|---|
| Verified batch rate | Batches that pass all required checks without manual edits / proposed batches | ≥ 80% |
| Orphan hit rate | PRs with ≥ 1 newly dead file / PRs analyzed | ≥ 30% |
| False-positive rate | Confirmed false positives / findings on the labeled set | ≤ 2% |
| Receipt completeness | Receipts with every required check recorded | 100% |
| Time to first receipt | Install → first verified receipt | ≤ 10 min |
| Design-partner retention | Teams still running Shrike after 30 days | ≥ 50% |
| Merged PR rate | Opened prune PRs that merge | ≥ 60% |

These are measured, not projected. The tracker records actuals and dates.

---

## 6. Adversarial testing

The must-not-report corpus covers known conventions. Adversarial testing covers
unknown ones:

- an agent-fabricated dynamic import;
- a runtime registry built from a glob;
- a barrel file generated at build time;
- a package export map with wildcards;
- a monorepo path alias that shadows a package name;
- a test that imports a symbol only through a string;
- a `"use client"` module imported only by a generated route;
- a server action invoked by a form in a different package.

The red-team prompt ([`../prompts/08-adversarial-red-team-agent.md`](../prompts/08-adversarial-red-team-agent.md))
owns this work. Every confirmed false positive becomes a corpus entry before
the fix lands.

---

## 7. Reporting

Public claims must link to:

- the raw benchmark JSON;
- the corpus version and result;
- the labeled-set version and precision table;
- the exact Shrike commit.

A claim without these is marketing, not evidence. The README may state a target;
only the tracker and the linked report may state a result.

---

## 8. Acceptance criteria

- [ ] The must-not-report corpus has ≥ 120 entries and zero findings.
- [ ] The labeled set has ≥ 10 repositories with two independent labelers.
- [ ] The calibration table reports per-rule precision with sample sizes.
- [ ] Benchmark reports record hardware, versions, cold/warm, and peak RSS.
- [ ] A 20% regression fails the benchmark job.
- [ ] Every public performance claim links to a raw report.
- [ ] A confirmed false positive becomes a corpus entry before the fix.
- [ ] The corpus runs with and without build evidence.
