# Shrike vs fallow, knip, depcheck, ts-prune, and ts-unused-exports — 12-fixture benchmark

**Date:** 2026-11-23 (run `2026-11-22T18:02:22.262546+00:00`)  
**Shrike:** 1.0.0, commit `35040c69f2c805f4cdc27c4f014cc06ef6049f68` + uncommitted working tree (UTF-8 HTML scanner fix)  
**Competitors:** fallow 3.46.0, knip 6.47.0, depcheck 1.4.7, ts-prune 0.10.3, ts-unused-exports 11.0.1  
**Machine:** AMD Ryzen 7 5700U with Radeon Graphics, 8 logical CPUs, WSL2 kernel 6.18.58.1-microsoft-standard-WSL2  
**Toolchain:** Rust, Node v24.21.0, TypeScript 5.9.3  
**Rounds:** 5 measured, 2 warmups  
**Load average:** [2.37841796875, 3.84521484375, 3.54248046875] at start

---

## 1. Bottom line

Across all 12 fixtures the Shrike CLI is faster than fallow in 11/12 cold scans (median 1.47×, geomean 1.45×) and 12/12 warm scans (median 1.93×, geomean 1.90×). The Rust engine is faster than fallow in 12/12 cold scans (median 3.33×, geomean 3.64×) and 12/12 warm scans (median 9.46×, geomean 10.14×).

Knip produced a parseable report on 10 fixtures; there the CLI is 1.27–27.06× faster cold (median 6.16×). Knip is unavailable on query and vite because its config loader imports unbuilt workspace or plugin packages; those configurations are excluded, not counted as slow scans.

The only CLI cold loss is vite (3.059 s vs 3.547 s). The engine remains faster there, so the gap is Node-side startup and JSON orchestration rather than analysis.

depcheck, ts-prune, and ts-unused-exports are reported separately because they do narrower work: on requo (1677 files) they take 27.496 s, 32.760 s, and 10.449 s cold.

Correctness was not traded for speed. The suite found and fixed a UTF-8 char-boundary panic in the HTML module-script scanner (Vite), no analysis rule changed, and every run is validated by exit code, parsed finding count, and output size.

On the labeled accuracy corpus (315 cases, framework-derived labels): shrike-cli 196 TP / 0 FP / 0 FN (P 100.0%, R 100.0%; 315/315 cases clean); fallow 180 TP / 68 FP / 16 FN (P 72.6%, R 91.8%; 278/315 cases clean); knip 143 TP / 82 FP / 47 FN (P 63.6%, R 75.3%; 257/314 cases clean). The corpus is a framework-convention gate, not a random repository sample; the per-category table is the right way to read it.

The machine is a shared WSL2 VM and is not CPU-pinned; paired interleaved rounds and medians reduce drift but do not eliminate it.

---

## 2. What was measured

The protocol is borrowed rather than invented.

**Fallow's public suite.** Fallow's
[`benchmarks/bench.mjs`](https://github.com/fallow-rs/fallow/blob/main/benchmarks/bench.mjs)
compares fallow with knip on pinned real-world projects and deterministic
synthetic projects. It runs five measured rounds after two warmups, measures a
cold configuration (`--no-cache`) and a warm cache, records median wall time and
peak RSS, counts issues from the JSON report, and drops a knip run that fails
instead of timing the error. This suite keeps those choices.

**Knip's performance guidance.** Knip's
[performance guide](https://knip.dev/guides/performance) recommends `--cache`
for consecutive runs and `--debug` / `--performance` for bottlenecks. Its large-scale references are
the ~10,000-file Sentry repository and the Astro monorepo. This suite measures
knip with and without `--cache` and includes 1,420-file and 901-file monorepos
plus a 5,001-file synthetic project.

**Added controls.** Every round runs all selected tools on the same fixture and
rotates the starting tool, so machine load affects each side of the comparison.
The Shrike CLI and the Rust engine are separate rows: the CLI includes Node
startup, configuration, the engine child, and output; the engine row isolates
the analyzer. The harness samples the whole process tree for RSS, but only after
`/proc/<pid>/exe` shows that the child has actually called `exec`: before that
point the child is a fork of the benchmark harness and its RSS is the harness's
own memory, not the tool's. Each sample is also checked against the PID's start
time so a reused PID cannot be attributed to the tool. `wait4()`'s `ru_maxrss`
is recorded for diagnosis but not used for the reported peak, because it
includes the pre-exec fork image too. The harness records p95, min, max, and
standard deviation as well as the median, and parses each report so a fast run
cannot win by producing no findings. A configuration whose exit code is outside
the tool's finding convention, or whose report cannot be parsed, is marked
invalid and excluded from comparisons.

The tools do not all do the same work. Shrike, fallow, and knip are full scans.
depcheck checks dependencies only; ts-prune and ts-unused-exports check exports
only. Those three are reported separately as narrow-scope context, never as
head-to-head full-scan wins.

### Corpus

| Fixture | Kind | Source | Revision | Source files |
|---|---|---|---|---:|
| portfolio | Next.js app | local copy | working tree | 23 |
| next-portfolio | Next.js app | local copy | working tree | 32 |
| markalvarez | Next.js app | local copy | working tree | 37 |
| accounteer | Next.js app | local copy | working tree | 676 |
| requo | Next.js app | local copy | working tree | 1677 |
| preact | library | https://github.com/preactjs/preact.git | 10.25.4 | 244 |
| fastify | library | https://github.com/fastify/fastify.git | v5.2.1 | 286 |
| zod | library | https://github.com/colinhacks/zod.git | v3.24.2 | 174 |
| query | monorepo | https://github.com/TanStack/query.git | v5.90.3 | 901 |
| vite | monorepo | https://github.com/vitejs/vite.git | v8.0.1 | 1420 |
| synthetic-1k | synthetic | benchmarks/generate.mjs | seed 42 | 1001 |
| synthetic-5k | synthetic | benchmarks/generate.mjs | seed 42 | 5001 |

The five Next.js apps are the local benchmark copies used in the previous report. The five OSS repositories are pinned revisions from fallow's real-world group. The synthetic projects use `benchmarks/generate.mjs` with seed 42, the same generator as the engine budgets.

### Tool commands

| Tool | Cold command | Warm command |
|---|---|---|
| Shrike CLI | `shrike scan --json` with `.shrike/cache` removed | same, cache retained |
| Shrike engine | `shrike-engine` with a JSON `analyze` request; cache dir removed | same, cache retained |
| fallow | `fallow dead-code --quiet --format json --no-cache` | same without `--no-cache` |
| knip | `knip --reporter json` | `knip --reporter json --cache` |
| depcheck | `depcheck --json --ignore-bin-package` | — |
| ts-prune | `ts-prune --project tsconfig.json --error` | — |
| ts-unused-exports | `ts-unused-exports tsconfig.json --maxIssues=0` | — |

---

## 3. Full-scan results

Median wall seconds and median peak RSS in MB. Ratios are competitor ÷ Shrike; above 1 means Shrike is faster.

### 3.1 Cold scans

| Fixture | Files | Shrike CLI | Fallow | S/F | Knip | S/K | Shrike RSS |
|---|---:|---:|---:|---:|---:|---:|---:|
| portfolio | 23 | 0.234 | 0.501 | 2.14× | 1.431 | 6.10× | 65.6 |
| next-portfolio | 32 | 0.218 | 0.414 | 1.90× | 1.497 | 6.87× | 63.1 |
| markalvarez | 37 | 0.213 | 0.405 | 1.90× | 1.294 | 6.07× | 63.0 |
| accounteer | 676 | 0.843 | 1.043 | 1.24× | 5.250 | 6.22× | 129.9 |
| requo | 1677 | 2.054 | 2.398 | 1.17× | 8.116 | 3.95× | 237.5 |
| preact | 244 | 0.399 | 0.669 | 1.68× | 10.792 | 27.06× | 79.0 |
| fastify | 286 | 0.404 | 0.651 | 1.61× | 2.633 | 6.51× | 83.2 |
| zod | 174 | 0.373 | 0.533 | 1.43× | 2.347 | 6.30× | 78.2 |
| query | 901 | 1.906 | 2.492 | 1.31× | — | — | 111.5 |
| vite | 1420 | 3.547 | 3.059 | 0.86× | — | — | 129.5 |
| synthetic-1k | 1001 | 0.310 | 0.470 | 1.52× | 1.036 | 3.34× | 76.2 |
| synthetic-5k | 5001 | 0.887 | 1.013 | 1.14× | 1.128 | 1.27× | 137.4 |

### 3.2 Warm scans

| Fixture | Files | Shrike CLI | Fallow | S/F | Knip `--cache` | S/K |
|---|---:|---:|---:|---:|---:|---:|
| portfolio | 23 | 0.208 | 0.624 | 2.99× | 1.449 | 6.96× |
| next-portfolio | 32 | 0.193 | 0.403 | 2.08× | 1.299 | 6.72× |
| markalvarez | 37 | 0.205 | 0.402 | 1.96× | 1.136 | 5.54× |
| accounteer | 676 | 0.464 | 0.814 | 1.76× | 1.583 | 3.41× |
| requo | 1677 | 0.846 | 1.254 | 1.48× | 2.050 | 2.42× |
| preact | 244 | 0.297 | 0.595 | 2.01× | 1.279 | 4.31× |
| fastify | 286 | 0.274 | 0.497 | 1.82× | 1.335 | 4.88× |
| zod | 174 | 0.254 | 0.489 | 1.92× | 1.150 | 4.52× |
| query | 901 | 1.107 | 2.236 | 2.02× | — | — |
| vite | 1420 | 1.729 | 2.583 | 1.49× | — | — |
| synthetic-1k | 1001 | 0.241 | 0.468 | 1.94× | 0.962 | 3.99× |
| synthetic-5k | 5001 | 0.574 | 0.959 | 1.67× | 1.110 | 1.93× |

### 3.3 Dispersion

Median / p95 / standard deviation for the largest fixtures.

| Fixture | Tool | Cold median | Cold p95 | Cold stdev | Warm median | Warm p95 | Warm stdev |
|---|---|---:|---:|---:|---:|---:|---:|
| accounteer | shrike-cli | 0.843 | 1.134 | 0.148 | 0.464 | 0.507 | 0.025 |
| accounteer | shrike-engine | 0.517 | 0.592 | 0.046 | 0.168 | 0.200 | 0.014 |
| accounteer | fallow | 1.043 | 1.183 | 0.067 | 0.814 | 1.024 | 0.108 |
| accounteer | knip | 5.250 | 5.632 | 0.331 | 1.583 | 1.943 | 0.190 |
| requo | shrike-cli | 2.054 | 2.310 | 0.249 | 0.846 | 0.868 | 0.016 |
| requo | shrike-engine | 1.601 | 2.429 | 0.415 | 0.422 | 0.423 | 0.017 |
| requo | fallow | 2.398 | 3.152 | 0.363 | 1.254 | 1.282 | 0.024 |
| requo | knip | 8.116 | 8.919 | 0.936 | 2.050 | 2.276 | 0.188 |
| query | shrike-cli | 1.906 | 2.005 | 0.056 | 1.107 | 1.194 | 0.043 |
| query | shrike-engine | 1.034 | 1.129 | 0.043 | 0.236 | 0.250 | 0.009 |
| query | fallow | 2.492 | 2.574 | 0.059 | 2.236 | 2.702 | 0.216 |
| vite | shrike-cli | 3.547 | 3.753 | 0.198 | 1.729 | 2.094 | 0.266 |
| vite | shrike-engine | 2.172 | 2.281 | 0.050 | 0.287 | 0.334 | 0.023 |
| vite | fallow | 3.059 | 3.202 | 0.073 | 2.583 | 2.893 | 0.141 |
| synthetic-1k | shrike-cli | 0.310 | 0.323 | 0.008 | 0.241 | 0.253 | 0.010 |
| synthetic-1k | shrike-engine | 0.128 | 0.159 | 0.015 | 0.057 | 0.059 | 0.002 |
| synthetic-1k | fallow | 0.470 | 0.513 | 0.022 | 0.468 | 0.479 | 0.011 |
| synthetic-1k | knip | 1.036 | 1.146 | 0.076 | 0.962 | 1.147 | 0.091 |
| synthetic-5k | shrike-cli | 0.887 | 0.944 | 0.032 | 0.574 | 0.652 | 0.036 |
| synthetic-5k | shrike-engine | 0.558 | 0.601 | 0.028 | 0.249 | 0.258 | 0.007 |
| synthetic-5k | fallow | 1.013 | 1.102 | 0.058 | 0.959 | 0.997 | 0.019 |
| synthetic-5k | knip | 1.128 | 1.264 | 0.082 | 1.110 | 1.274 | 0.102 |

### 3.4 Ratios

| Comparison | Fixtures | Median | Geomean | Min | Max | Shrike wins |
|---|---:|---:|---:|---:|---:|---:|
| Fallow cold ÷ Shrike CLI cold | 12 | 1.47× | 1.45× | 0.86× | 2.14× | 11/12 |
| Fallow warm ÷ Shrike CLI warm | 12 | 1.93× | 1.90× | 1.48× | 2.99× | 12/12 |
| Fallow cold ÷ Shrike engine cold | 12 | 3.33× | 3.64× | 1.41× | 13.51× | 12/12 |
| Fallow warm ÷ Shrike engine warm | 12 | 9.46× | 10.14× | 2.97× | 39.07× | 12/12 |

---

## 4. Engine results

| Fixture | Files | Engine cold | Fallow cold | E/F | Engine warm | Fallow warm | E/F | Engine RSS |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| portfolio | 23 | 0.051 | 0.501 | 9.87× | 0.016 | 0.624 | 39.07× | 11.7 |
| next-portfolio | 32 | 0.034 | 0.414 | 12.04× | 0.015 | 0.403 | 26.44× | 9.2 |
| markalvarez | 37 | 0.030 | 0.405 | 13.51× | 0.016 | 0.402 | 25.63× | 9.1 |
| accounteer | 676 | 0.517 | 1.043 | 2.02× | 0.168 | 0.814 | 4.83× | 67.3 |
| requo | 1677 | 1.601 | 2.398 | 1.50× | 0.422 | 1.254 | 2.97× | 158.2 |
| preact | 244 | 0.148 | 0.669 | 4.51× | 0.053 | 0.595 | 11.21× | 24.4 |
| fastify | 286 | 0.203 | 0.651 | 3.20× | 0.053 | 0.497 | 9.43× | 27.7 |
| zod | 174 | 0.154 | 0.533 | 3.46× | 0.045 | 0.489 | 10.94× | 22.7 |
| query | 901 | 1.034 | 2.492 | 2.41× | 0.236 | 2.236 | 9.48× | 50.6 |
| vite | 1420 | 2.172 | 3.059 | 1.41× | 0.287 | 2.583 | 8.99× | 57.9 |
| synthetic-1k | 1001 | 0.128 | 0.470 | 3.67× | 0.057 | 0.468 | 8.23× | 20.4 |
| synthetic-5k | 5001 | 0.558 | 1.013 | 1.82× | 0.249 | 0.959 | 3.85× | 71.2 |

---

## 5. Narrow-scope tools

These tools do less work than a full scan: depcheck checks dependencies only; ts-prune and ts-unused-exports check exports only. Their times are context, not a head-to-head ranking.

| Fixture | Files | Depcheck | ts-prune | ts-unused-exports |
|---|---:|---:|---:|---:|
| portfolio | 23 | 2.548 | 5.304 | 1.061 |
| next-portfolio | 32 | 2.221 | 5.098 | 0.952 |
| markalvarez | 37 | 2.068 | 4.902 | 0.901 |
| accounteer | 676 | 13.724 | 18.497 | 6.309 |
| requo | 1677 | 27.496 | 32.760 | 10.449 |
| zod | 174 | 5.625 | 12.049 | 1.234 |

Narrow-scope configuration: 3 measured rounds, 1 warmup.

---

## 6. Validation

Every measured run records its exit code, parsed finding count, and output size. A configuration outside the tool's finding convention is invalid and is excluded from the tables above.

| Fixture | Tool | Mode | Exit | Findings | Wall seconds |
|---|---|---|---:|---:|---:|
| query | knip | cold | 2 | 238 | 4.307 |
| query | knip | warm | 2 | 238 | 2.453 |
| vite | knip | cold | 2 | 1226 | 11.718 |
| vite | knip | warm | 2 | 1226 | 6.972 |

### Finding counts (cold)

| Fixture | Shrike CLI | Shrike engine | Fallow | Knip |
|---|---:|---:|---:|---:|
| portfolio | 1 | 1 | 2 | 6 |
| next-portfolio | 13 | 11 | 3 | 5 |
| markalvarez | 27 | 27 | 5 | 4 |
| accounteer | 193 | 192 | 319 | 380 |
| requo | 743 | 737 | 929 | 1056 |
| preact | 61 | 43 | 41 | 177 |
| fastify | 6 | 0 | 21 | 76 |
| zod | 21 | 17 | 150 | 119 |
| query | 315 | 244 | 183 | — |
| vite | 204 | 0 | 537 | — |
| synthetic-1k | 1182 | 1182 | 1001 | 1001 |
| synthetic-5k | 5990 | 5990 | 5001 | 5001 |

Shrike CLI counts include dependency findings; the engine counts graph candidates and unused files only. On fastify and vite the CLI's 6 and 204 findings are dependency findings only, so the engine's 0 is consistent rather than a missing scan.

---

## 7. Correctness and accuracy

The benchmark is also a correctness probe, not only a stopwatch.

- The first full run crashed Shrike on Vite with a UTF-8 char-boundary panic in
  `crates/shrike-syntax/src/parse/html.rs` (`end byte index 7 is not a char
  boundary; it is inside 'テ'`). The scanner compared a fixed-length prefix with
  `text[..prefix.len()]`, which can split a multi-byte character. It now compares
  bytes, and `handles_non_ascii_text_at_a_script_prefix_boundary` covers the
  case. Vite now produces a report instead of a panic.
- Every measured configuration records its exit code, parsed finding count, and
  output size. A configuration outside the tool's finding convention is marked
  invalid and excluded. The full validation table is in the raw summary.
- No analysis rule changed. The UTF-8 fix only replaces a slice that could
  panic with an equivalent byte comparison; inputs that previously parsed
  successfully produce the same result.
- The accuracy agreement study remains the one in
  [`competitor-2026-11-22.md`](competitor-2026-11-22.md): outside unused files,
  Shrike-only findings were 0/0/0/24/26 on the five Next.js apps, and every
  disagreement was inspected. This run adds OSS-library coverage and the
  performance numbers; it does not re-label the corpus.

The separate 315-case labeled run shows where the tools diverge. Fallow's false
positives cluster in component-workbench conventions (React Cosmos, Ladle), test
entries, Expo routes, and Next.js segment/config surfaces; its misses cluster in
Next.js Pages extensions, worker entries, and Gatsby templates. Knip's false
positives cluster in Metro platform files, test entries, Remix/React Router
routes, and config entries; its misses cluster in Next.js segment config, Expo
routes, Pages Router extensions, and Gatsby templates. The export-only tools
false-positive heavily on route conventions (parallel routes, intercepting
routes, route groups) because they do not model framework entries. Shrike's
315/315 is expected because this corpus is its own CI gate; the competitor
scores on the same labels are the informative comparison.

### Labeled-corpus accuracy

The performance suite checks that a report is produced; this suite checks whether the findings are right. `fixtures/must-not-report` contains 315 minimal projects with framework-derived labels: protected symbols/files that must never be reported dead, and must-report symbols/files that must be found. A symbol counts as found if the tool reports it or reports its whole file unused. Used-file false positives use a closure allowance (a file whose every importer is also reported unused is transitively dead); the strict count is in the standalone report.

| Tool | Scope | Valid cases | TP | FP | FN | Precision | Recall | F1 | Cases clean |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| shrike-cli | symbols + files | 315 | 196 | 0 | 0 | 100.0% | 100.0% | 100.0% | 315/315 |
| fallow | symbols + files | 315 | 180 | 68 | 16 | 72.6% | 91.8% | 81.1% | 278/315 |
| knip | symbols + files | 314 | 143 | 82 | 47 | 63.6% | 75.3% | 68.9% | 257/314 |
| ts-prune | symbols | 315 | 94 | 183 | 96 | 33.9% | 49.5% | 40.3% | 131/315 |
| ts-unused-exports | symbols | 283 | 102 | 160 | 18 | 38.9% | 85.0% | 53.4% | 167/283 |

Invalid cases (tool could not produce a report): knip 1, ts-unused-exports 32. They are excluded from that tool's metrics.

Full per-case results, including every false positive and every missed must-report item, are in [`competitor-accuracy-2026-11-23.md`](competitor-accuracy-2026-11-23.md).

---

## 8. Caveats

- The machine is a shared WSL2 VM, not CPU-pinned. The load average at the
  start of the measured run is recorded in the raw JSON. Paired, interleaved
  rounds and medians reduce drift; absolute times from different runs are not
  directly comparable.
- All wall times, finding counts, validity flags, and peak-RSS values in this
  report come from one clean run, `2026-11-22T18:02:22Z`
  (`/tmp/opencode/bench-v2/results-full`). No other benchmark process was
  running. Peak RSS is sampled from the process tree only after
  `/proc/<pid>/exe` differs from the harness, and each sample is checked
  against the PID start time. `wait4()`'s `ru_maxrss` is recorded for
  diagnosis but not used for the reported peak because it includes the
  pre-exec fork image.
- Cold and warm are tool-specific. Shrike removes `.shrike/cache`; fallow uses
  `--no-cache` when cold and its default cache when warm; knip runs without
  `--cache` when cold and with `--cache` when warm. A warm number is never
  presented as cold.
- The OSS fixtures are source checkouts, not production builds. The Next.js
  apps, preact, fastify, and zod have installed dependencies; query and vite
  are missing packages their configs import. Shrike and fallow scan the source
  tree without dependencies; knip can do the same unless a config file imports
  an unbuilt workspace or plugin package. Knip exits 2 on query and vite for
  that reason; those configurations are marked invalid and excluded from the
  full-scan comparison. Zod is valid after installing its config's `vite@5`
  peer (`5.4.21`), so knip is compared on 10/12 fixtures and is much slower
  than Shrike on every one.
- Finding counts validate that a run did work; they are not an accuracy score.
  The tools report different categories (for example, Shrike's `internalOnly`
  entries are advisory and knip/fallow suppress exports inside unused files).
  Shrike CLI counts include dependency findings, while the engine row counts
  graph candidates and unused files only; on fastify and vite the CLI's 6 and
  204 findings are dependency findings only, so the engine's 0 is consistent.
  Accuracy is measured separately on the 315-case labeled corpus in
  [`competitor-accuracy-2026-11-23.md`](competitor-accuracy-2026-11-23.md);
  dependency accuracy remains the agreement study in
  [`competitor-2026-11-22.md`](competitor-2026-11-22.md).
- The analyzed binary is the working tree at the recorded commit. The UTF-8
  char-boundary fix in the HTML module-script scanner is the only analysis
  change made in response to this benchmark; it removes a crash and leaves
  every previously successful parse unchanged.
- depcheck, ts-prune, and ts-unused-exports do less work than a full scan. They
  are context, not a ranking.

---

## 9. Reproduction

```bash
# Build Shrike
cargo build --release -p shrike-engine
pnpm --filter shrike build

# Prepare the corpus and competitor tools
benchmarks/competitors/setup.sh

# Full-scan suite (5 measured rounds, 2 warmups)
python3 benchmarks/competitors/bench.py --work /tmp/opencode/bench-v2 \
  --tools shrike-cli,shrike-engine,fallow,knip \
  --out /tmp/opencode/bench-v2/results-full

# Narrow-scope suite
python3 benchmarks/competitors/bench.py --work /tmp/opencode/bench-v2 \
  --tools depcheck,ts-prune,ts-unused-exports \
  --fixture portfolio,next-portfolio,markalvarez,accounteer,requo,zod --quick

# Labeled accuracy suite (315 cases)
python3 benchmarks/competitors/accuracy.py --work /tmp/opencode/bench-v2
```

Raw results:

- `/tmp/opencode/bench-v2/results-full/summary.json`
- `/tmp/opencode/bench-v2/results-full/summary.md`
- `/tmp/opencode/bench-v2/results-narrow/summary.json`
- `/tmp/opencode/bench-v2/results-accuracy/summary.json`
- `/tmp/opencode/bench-v2/results-accuracy/summary.md`

