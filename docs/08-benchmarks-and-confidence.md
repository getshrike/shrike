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

The 2026-11-23 competitor suite in §4.7 uses 12 fixtures: five local Next.js
apps, five pinned OSS repositories (preact, fastify, zod, query, vite), and two
synthetic projects (1,001 and 5,001 source files). The 315-case labeled corpus
in `fixtures/must-not-report` is a separate accuracy gate, not a timing corpus.

### 4.3 Method

- 3 warmups, 5 measured runs for the engine harness. The competitor suite
  follows fallow's public protocol: 2 warmups and 5 measured runs.
- Report median, p95, and peak RSS.
- Cold: cache removed before each run.
- Warm: cache present and valid.
- No other workload on the machine.
- Pin CPU frequency if possible; record if not.
- Run the engine directly, not through a shell wrapper.
- Measure wall time and peak RSS with the harness. Sample the process tree only
  after `/proc/<pid>/exe` shows the child has called `exec`, and do not use
  `wait4`'s `ru_maxrss` for the reported peak: before `exec` the child is a fork
  of the harness, so both readings can include the harness's own memory.

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

### 4.7 Competitor comparison

**2026-11-23 labeled accuracy.** The 315-case
[`competitor-accuracy-2026-11-23.md`](../benchmarks/competitor-accuracy-2026-11-23.md)
run scores findings against framework-derived labels rather than against
another tool's output. Shrike scored 196 TP / 0 FP / 0 FN (315/315 cases clean);
fallow 180/68/16 (278/315); knip 143/82/47 (257/314); ts-prune 94/183/96
(131/315); ts-unused-exports 102/160/18 (167/283). Shrike's perfect score is
expected because this corpus is its own CI gate, not an independent sample; the
competitor scores on the same labels are the informative comparison. The
false-positive clusters are convention-specific: React Cosmos/Ladle and test
entries for fallow, Metro and Remix/React Router for knip, and route conventions
for the export-only tools.

The 2026-11-22 comparison against knip 5.88.1, fallow 3.46.0, and depcheck
1.4.7 is recorded in
[`benchmarks/competitor-2026-11-22.md`](../benchmarks/competitor-2026-11-22.md).
It uses three small Next.js apps (22–36 analyzed files) and two production-sized
apps (675 and 1,673 files), release builds, a warm OS cache, paired interleaved
rounds (each round runs Shrike and fallow in alternating order), five-round
medians, and peak RSS from `wait4`.

| Workload | Shrike CLI cold | Shrike engine cold | Fallow cold | Knip | Depcheck |
|---|---:|---:|---:|---:|---:|
| 22 files | 0.233 s / 58 MB | 0.056 s / 15 MB | 0.517 s / 109 MB | 2.92 s / 204 MB | 2.65 s / 206 MB |
| 1,673 files | 1.773 s / 158 MB | 1.505 s / 158 MB | 1.979 s / 162 MB | 15.15 s / 677 MB | 26.10 s / 448 MB |
| 675 files | 0.796 s / 85 MB | 0.464 s / 66 MB | 0.936 s / 109 MB | 9.68 s / 520 MB | 13.19 s / 455 MB |

The Shrike CLI beats fallow cold on every repository: 1.12× on the 1,673-file
app, 1.18× on the 675-file app, and 1.80–2.22× on the small apps. It is
1.48–2.12× faster warm. The Shrike engine's margin is wider: 1.32–16.4× cold and
3.24–30.5× warm. Across all five repositories the Shrike CLI is 8.5–12.5×
faster than knip and 9.1–16.6× faster than depcheck cold; against knip's cache
it is 10.8–16.0× faster warm. On requo the CLI's warm overhead over the engine
is ~0.49 s (0.910 s vs 0.417 s), and peak RSS is 122 MB. The engine output is
byte-identical before and after the optimization, and the CLI's parsed JSON is
identical on all five repositories.

Accuracy is an agreement study, not precision/recall. Outside files reported
unused by any tool, Shrike's combined exported-plus-advisory set had 0, 0, 0, 24,
and 26 findings not shared with knip/fallow. The 24 on requo are 2 genuine
transitive dead exports, 1 dead re-export, and 21 advisory de-export-only
entries; the 26 on accounteer are 13 server actions that fallow also reports, 3
dead analytics exports (two declarations and one unused re-export), and 10
advisory entries. Knip had 13 and 12 outside-unused-file differences on requo
and accounteer; fallow had none on either. Shrike and fallow agree on 25/25 and
23/23 server actions. Shrike matched knip on both repositories with real unused
dependencies and reported no config-only false positives on the other three.

The comparison's caveats are in the report: no complete ground truth, different
finding categories (`internalOnly` is advisory), knip/fallow suppress exports in
unused files, the absent-prop rule is warning-only, and the remaining
conservative gap is opaque namespace dynamic imports. The earlier
default-exported-const absent-prop gap is closed, and returned loaders and
property assertions are now narrowed (D-048/D-049).

The expanded 2026-11-23 comparison is recorded in
[`benchmarks/competitor-2026-11-23.md`](../benchmarks/competitor-2026-11-23.md).
It replaces the small hand-picked corpus with 12 paired fixtures: five Next.js
apps, the same pinned OSS repositories fallow uses (preact, fastify, zod,
query, vite), and two generated projects (1k and 5k files). It keeps fallow's
2-warmup / 5-measured-round, cold-and-warm, median-plus-RSS protocol and adds
finding-count validation, invalid-run exclusion, separate CLI and engine rows,
and rotating tool order. depcheck, ts-prune, and ts-unused-exports are measured
as narrow-scope context rather than ranked against full scans. The suite is
owned by the repository under `benchmarks/competitors/`.

On the clean 2026-11-22T18:02:22Z run, the Shrike CLI beat fallow cold on 11/12
fixtures (median 1.47×, geomean 1.45×) and warm on 12/12 (1.93× / 1.90×). The
engine beat fallow cold on 12/12 (3.33× / 3.64×) and warm on 12/12
(9.46× / 10.14×). Knip was valid on 10/12; there the CLI was 1.27–27.06× faster
cold (median 6.16×). The only CLI cold loss was Vite (0.86×), where the engine
still beat fallow (1.41×), so the gap is Node-side startup and JSON
orchestration rather than analysis. On requo (1,677 files) the CLI measured
2.054 s cold / 0.846 s warm, the engine 1.601 / 0.422, and fallow 2.398 / 1.254.

The benchmark also acts as a correctness probe: the first full run found a
UTF-8 char-boundary panic in the HTML module-script scanner on Vite. The fix is
byte-based prefix matching plus a regression test, and no analysis rule
changed. RSS sampling now starts only after the child has called `exec` and
verifies each PID's start time, so neither the harness's pre-exec fork image
nor a reused PID can attribute another process's memory to a fast tool.
Knip configurations that exit 2 because their configs import unbuilt workspace
or plugin packages (query and vite) are excluded rather than timed as slow
scans; zod becomes valid after installing its config's `vite@5` peer.

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
