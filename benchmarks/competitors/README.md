# Competitor benchmark

This directory holds a reproducible, paired benchmark for Shrike against the
main JavaScript/TypeScript dead-code tools. It is separate from the synthetic
engine harness in `benchmarks/harness.mjs`: this suite measures the whole CLI
and its competitors on real repositories.

## Method

The protocol follows the public benchmark suites rather than inventing a new
one:

- **fallow's benchmark suite** (`fallow-rs/fallow/benchmarks/bench.mjs`) compares
  fallow with knip on pinned real-world projects and deterministic synthetic
  projects. It runs cold and warm cache configurations, does warmups, reports
  medians and peak RSS, and records issue counts so a fast run cannot win by
  producing no findings.
- **knip's performance guide** recommends `--cache` for consecutive runs and
  uses a ~10,000-file Sentry repository and the Astro monorepo as its
  large-scale references.
- This suite adds **paired/interleaved rounds**: in each round every tool runs
  on the same fixture, with the starting order rotated. Load drift therefore
  affects all tools rather than one side of the comparison.
- **Cold** means the tool's cache is removed before every measured run.
  **Warm** means the cache is populated once, then reused. Both are reported
  separately; a warm run is never presented as cold.
- Default measurement is **2 warmups and 5 measured rounds**, with median, p95,
  min, max, mean, standard deviation, and peak RSS. The harness samples the
  whole process tree so wrapper processes and engine children are counted.
  Sampling starts only after `/proc/<pid>/exe` shows the child has called
  `exec`; before that the child is a fork of the harness and its RSS is the
  harness's own memory. Each sample also verifies the process start time, so a
  reused PID cannot contribute an unrelated process's RSS. `wait4()`'s
  `ru_maxrss` is recorded for diagnosis but not used for the reported peak,
  because it includes the pre-exec fork image.
- Every run is **validated**: the exit code must match the tool's finding
  convention, JSON output is parsed, and the finding count and output size are
  recorded. A configuration whose exit code is outside the tool's finding
  convention, or whose report cannot be parsed, is marked invalid and is not
  compared. This mirrors fallow's suite, which drops failed knip runs rather
  than timing an error.

The tools do not all do the same work. The headline tables compare full scans:
Shrike, fallow, and knip. `depcheck` checks dependencies only; `ts-prune` and
`ts-unused-exports` check exports only. They are reported separately as
narrow-scope context, not as head-to-head full-scan wins.

The comparison uses fallow 3.46.0, knip 6.47.0, depcheck 1.4.7, ts-prune
0.10.3, and ts-unused-exports 11.0.1. The 2026-11-23 report records the exact
versions and machine in its header.

## Corpora

| Fixture | Kind | Source | Revision |
|---|---|---|---|
| portfolio | Next.js app | local benchmark copy | working tree |
| next-portfolio | Next.js app | local benchmark copy | working tree |
| markalvarez | Next.js app | local benchmark copy | working tree |
| accounteer | Next.js app | local benchmark copy | working tree |
| requo | Next.js app | local benchmark copy | working tree |
| preact | library | https://github.com/preactjs/preact | `10.25.4` |
| fastify | library | https://github.com/fastify/fastify | `v5.2.1` |
| zod | library | https://github.com/colinhacks/zod | `v3.24.2` |
| query | monorepo | https://github.com/TanStack/query | `v5.90.3` |
| vite | monorepo | https://github.com/vitejs/vite | `v8.0.1` |
| synthetic-1k | synthetic | `benchmarks/generate.mjs`, seed 42 | 1,000 source files |
| synthetic-5k | synthetic | `benchmarks/generate.mjs`, seed 42 | 5,000 source files |

The OSS fixtures are the same pinned revisions fallow uses for its small and
large real-world groups. Dependencies are installed with lifecycle scripts
disabled, matching fallow's download script. The synthetic projects are the
same generator used for Shrike's engine budgets, so scaling claims have a
stable input.

## Accuracy

Speed is only half of the comparison. `accuracy.py` runs the same tools over
`fixtures/must-not-report`, a 315-case labeled corpus with framework-derived
expectations: protected symbols and files that must never be reported dead, and
must-report symbols and files that must be found. Scoring is at the labeled item
level (TP/FP/FN, precision, recall, F1), with a closure allowance for used files
whose every importer is also reported unused. The narrow tools are scored on
symbols only because they do not report files. The corpus is also Shrike's own
CI false-positive gate, so Shrike's perfect score is expected by construction
and is not independent evidence; the competitor scores on the same labels are
the informative comparison. The full per-case report is
`benchmarks/competitor-accuracy-2026-11-23.md`.

```bash
python3 benchmarks/competitors/accuracy.py --work /tmp/opencode/bench-v2
```

## Setup

```bash
# Shrike release binaries and CLI
cargo build --release -p shrike-engine
pnpm --filter shrike build

# Clone the pinned OSS fixtures, install their dependencies, generate the
# synthetic projects, and install the competitor tools.
benchmarks/competitors/setup.sh

# Or point at an existing work directory / Next.js corpus:
SHRIKE_BENCH_WORK=/tmp/opencode/bench-v2 \
SHRIKE_BENCH_NEXT_REPOS=/tmp/opencode/bench-repos \
  benchmarks/competitors/setup.sh
```

## Run

```bash
# Full suite (2 warmups, 5 measured rounds)
python3 benchmarks/competitors/bench.py --work /tmp/opencode/bench-v2

# Quick smoke run
python3 benchmarks/competitors/bench.py --quick --fixture requo,vite

# One tool group
python3 benchmarks/competitors/bench.py \
  --work /tmp/opencode/bench-v2 \
  --tools shrike-cli,fallow,knip \
  --fixture requo,accounteer
```

Results are written to `<work>/results/summary.json` and
`<work>/results/summary.md`. Raw command output is deleted after validation
unless `--keep-raw` is passed.

If the command runner limits wall time, run the suite in chunks and merge the
summaries; each fixture still gets the full paired protocol:

```bash
python3 benchmarks/competitors/merge_results.py --out <work>/results-full \
  <work>/results-final-A <work>/results-final-B
```

The merged metadata records each chunk's fixture list and load averages.

## Caveats

- The benchmark machine is shared and not CPU-pinned. Record the load average
  printed in the metadata; paired rounds reduce, but do not eliminate, drift.
- Cold/warm cache semantics differ by tool. Shrike removes `.shrike/cache`,
  fallow uses `--no-cache`, knip uses no `--cache` cold and `--cache` warm.
- Findings are compared only for validation. Accuracy is measured separately
  on the labeled corpus in `benchmarks/competitor-accuracy-2026-11-23.md`;
  dependency accuracy remains the agreement study in
  `benchmarks/competitor-2026-11-22.md`.
- The OSS fixtures are source checkouts with installed dependencies where the
  registry allowed it, not production builds. This measures static analysis,
  not `next build`. When a fixture has no installed dependencies, knip can
  still scan it unless one of its config files imports an unbuilt workspace
  or plugin package; those runs are marked invalid and excluded from the
  comparison. In the 2026-11-23 run knip is valid on 10/12 fixtures and
  unavailable on query and vite for this reason.
- A run with an invalid configuration is shown as `—` in the summary tables and
  is listed under "Invalid configurations" in the raw analysis.
