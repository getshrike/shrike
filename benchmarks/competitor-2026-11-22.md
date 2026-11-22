# Shrike vs knip, fallow, and depcheck — five Next.js repositories

**Date:** 2026-11-22  
**Shrike:** 1.0.0, commit `35040c6` (working tree uncommitted)  
**Competitors:** knip 5.88.1, fallow 3.46.0 (signed npm binary), depcheck 1.4.7  
**Machine:** AMD Ryzen 7 5700U (4 cores / 8 threads), 5.8 GiB RAM, WSL2 Arch Linux, kernel 6.18.58.1, ext4  
**Toolchain:** Rust 1.100.0, Node 24.21.0, pnpm 12.18.0

---

## 1. Bottom line

**Performance**

- The Shrike CLI now beats fallow on every cold and warm scan in a paired,
  interleaved suite. On the two large repositories it is 1.12× (requo, 1,673
  files) and 1.18× (accounteer, 675 files) faster cold, and 1.48× and 1.57×
  faster warm. On the three small repositories it is 1.80–2.22× faster cold and
  2.00–2.12× faster warm.
- The Shrike **engine** is faster than fallow in every configuration: 1.32×
  (requo) and 2.02× (accounteer) cold, 3.24× and 4.66× warm, and 9.3–16.4×
  cold / 26.2–30.5× warm on the small applications.
- The Shrike CLI is 8.5–12.5× faster than knip and 9.1–16.6× faster than
  depcheck cold on every repository. Against knip's cache it is 10.8–16.0×
  faster warm; the cache helped on four of five repositories and was
  neutral-to-slower on the 22-file app.
- The large-scan gap was closed on both sides of the engine boundary. The
  engine compiles discovery globs once, runs the independent finish passes
  concurrently, sorts edges in parallel, uses a hash-set BFS for reachability,
  skips enum work when a repository has no enums, and emits a compact
  dependency projection instead of making the CLI decode the full snapshot. The
  CLI lazily imports only the command being run, writes the engine's snapshot
  bytes straight to stdout, and filters dependency candidates with literal
  substring checks and shared directory/alias caches. On requo the CLI's warm
  overhead over the engine is ~0.49 s (0.910 s vs 0.417 s), and peak RSS fell
  from 339 MB to 122 MB.
- Accuracy is unchanged. The engine's JSON is byte-identical to the pre-change
  engine on all five repositories, and the CLI's parsed JSON is identical on all
  five. The benchmark-driven accuracy fixes from the previous revision are
  included.

**Accuracy**

- Outside files reported unused by any tool, Shrike's combined
  exported-candidate + advisory set had 0, 0, 0, 24, and 26 findings not shared
  with knip/fallow on portfolio, next-portfolio, markalvarez, requo, and
  accounteer. Those are not 24/26 dead symbols:
  - requo: 2 genuine transitive dead exports, 1 dead re-export
    (`lib/deployment.ts#DeploymentMode`), and 21 advisory de-export-only
    entries;
  - accounteer: 13 server actions that fallow also reports, 3 dead analytics
    exports (`clientAnalyticsConfigured`, `currentConsentState`, and the
    `CONSENT_REOPEN_EVENT` re-export), and 10 advisory entries.
- Shrike caught two transitive dead exports on requo that knip and fallow left
  live because they saw an import from a dead module:
  `lib/ai/model-options.ts#getModelsForProvider` and
  `lib/ai/types.ts#AiRoutingProfile`.
- The returned-import and assertion narrowing exposed dead exports that the
  previous namespace-widening rule hid: requo's `lib/deployment.ts` re-export
  surface, and accounteer's three analytics exports plus
  `lib/auth/workspace-actions.ts#createEmployerWorkspaceAction`. The last one is
  a former Shrike/fallow false negative: the module is dynamically imported in a
  test whose only uses are negative `not.toHaveProperty` assertions, which
  consume no exports; the action's only other mentions are string assertions in
  source-text tests, not symbol references.
- Knip had 13 outside-unused-file differences on requo and 12 on accounteer;
  fallow had 0 on both. All inspected differences were dead-export or de-export
  candidates, type-shape visibility differences, or competitor false positives.
  None was a Shrike false positive on a deletion candidate.
- Shrike and fallow agree 25/25 on requo server actions and 23/23 on accounteer.
  Knip reports all 25 requo actions but only 10/23 accounteer actions.
- Shrike reported no unused dependencies on portfolio, markalvarez, or
  accounteer. The competitors' extra findings there were config-only false
  positives: knip/fallow/depcheck on portfolio and markalvarez, and depcheck on
  accounteer. Shrike matched knip exactly on next-portfolio and requo, and found
  the most unlisted dependencies.
- React absent-prop warnings are warning-only. Shrike now matches fallow exactly
  on portfolio's 33 props (19 `HalftoneReveal` plus 14 `Folder`), and finds 2
  additional true positives on markalvarez. On requo/accounteer Shrike reports
  197/80 warnings while fallow reports 0; that is a fallow recall difference,
  not a Shrike precision claim.

**Main caveats**

- These are agreement counts plus inspected disagreements, not precision/recall:
  there is no complete ground truth for a 1,673-file application.
- Shrike's `internalOnly` list is advisory and is not a deletion candidate. Knip
  and fallow's "unused exports" include de-export candidates. The tools are not
  reporting identical categories.
- The remaining conservative misses are opaque namespace uses (a bare,
  exported, or computed dynamic-import namespace) that still keep a whole
  module's export surface live. Returned loaders and property assertions are now
  narrowed; aliases and cross-module loaders are not.
- The benchmark machine was shared and is not CPU-pinned. The clean 19:12
  suite used here ran while the machine was mostly idle; an earlier 17:xx suite
  was discarded because an unrelated Playwright run overlapped it. WSL2 timing
  is still noisier than bare metal, so individual wall times should be read with
  that in mind.

---

## 2. Repositories

Copies were made under `/tmp/opencode/bench-repos/` with `node_modules`
symlinked from the original working copies. The copies have no `.git`, so commit
pinning is unavailable; the Next.js version and package name are recorded
instead.

| Repository | Package | Next.js | Shrike files | Lines | Symbols | Raw TS/JS files | Notes |
|---|---|---:|---:|---:|---:|---:|---|
| portfolio | `portfolio` | 16.4.2 | 22 | 5,871 | 241 | 23 | ignores `next-env.d.ts` |
| next-portfolio | `new-portfolio` | 16.4.2 | 31 | 3,001 | 156 | 32 | ignores `next-env.d.ts` |
| markalvarez | `markalvarez` | 16.4.6 | 36 | 3,111 | 157 | 37 | ignores `next-env.d.ts` |
| requo | `requo` | 16.4.6 | 1,673 | 315,629 | 12,247 | 1,677 | ignores `next-env.d.ts` and `coverage/*.js` |
| accounteer | `accounteer` | 16.3.6 | 675 | 112,794 | 4,720 | 676 | ignores `next-env.d.ts` |

The first three are small App Router applications. requo and accounteer are
production-sized applications with server actions, route handlers, tests, and
scripts. All five use the App Router.

---

## 3. Method

### 3.1 Builds and commands

Shrike was built in release mode:

```bash
cargo build --release -p shrike-engine
pnpm --filter shrike build
```

The CLI was run from its built `dist` entry point with the release engine
selected explicitly:

```bash
SHRIKE_ENGINE=/home/mark/code/projects/shrike/target/release/shrike-engine \
  node /home/mark/code/projects/shrike/packages/cli/dist/main.js scan --json
```

The engine was also measured directly, with a JSON request on stdin:

```bash
/home/mark/code/projects/shrike/target/release/shrike-engine \
  < /tmp/opencode/bench/engine-request-requo.json
```

Competitor commands:

```bash
# knip, default (no cache)
node /tmp/opencode/bench-tools/node_modules/knip/bin/knip.js \
  --no-progress --no-exit-code --reporter json

# knip, cached
node /tmp/opencode/bench-tools/node_modules/knip/bin/knip.js \
  --cache --cache-location /tmp/opencode/bench/knip-cache-requo \
  --no-progress --no-exit-code --reporter json

# fallow, cold
/tmp/opencode/bench-tools/node_modules/.bin/fallow dead-code --no-cache -f json -q

# fallow, warm
/tmp/opencode/bench-tools/node_modules/.bin/fallow dead-code -f json -q

# depcheck
node /tmp/opencode/bench-tools/node_modules/depcheck/bin/depcheck.js \
  --json --ignore-bin-package
```

Exit codes are expected to be non-zero when findings exist: Shrike and fallow
exit 1, depcheck exits 255. The harness records the exit code but does not treat
it as a failure.

### 3.2 Timing harness

`/tmp/opencode/bench/paired.py` runs Shrike and fallow in the same round,
alternating which tool goes first, and uses `os.wait4` to record wall time and
the peak RSS of the whole process tree. Interleaving makes both tools share the
same load window instead of comparing a quiet Shrike run with a busy fallow run.
Wall time includes process startup and JSON output. Peak RSS includes the Node
runtime for Node-based tools.

- Five measured rounds per configuration, median reported. Each round runs one
  Shrike configuration and one fallow configuration in alternating order.
- Warm configurations use one warmup run to populate each tool's cache.
- Cold Shrike CLI runs remove `.shrike/cache` before every measured run; cold
  engine runs remove the engine cache directory; fallow cold uses `--no-cache`;
  knip default does not use its cache.
- The final performance table is the paired suite run at 23:12–23:15 on
  2026-11-22. Unrelated work kept the load average around 3–6, so absolute
  times are higher than the earlier idle suite. Both tools see the same rounds,
  so the ratios are the like-for-like comparison.
- Knip and depcheck were measured in the earlier suite and were not re-run in
  the paired suite; their timings are kept for the cross-tool context and their
  ratios in §4.3 use the new Shrike medians.
- The OS page cache was warm for all runs. No CPU pinning was used. WSL2 timing
  is noisier than bare-metal timing.
- The engine was invoked directly with the request JSON on stdin; the harness
  adds no shell.

### 3.3 Accuracy normalization

Symbols are normalized as `file#name`. The comparison is:

| Shrike | Knip | Fallow |
|---|---|---|
| exported candidates + `internalOnly` | `exports` + `types` | `unused_exports` + `unused_types` |

Rules:

- `internalOnly` is **advisory**: exported but only referenced from its own
  file. It is not a deletion candidate. It is included in the combined set so
  that knip/fallow de-export findings are not counted as disagreements.
- Knip and fallow suppress exports inside files they report as unused. Shrike
  reports symbols individually. Symbols inside **any** tool's unused files are
  therefore counted separately and excluded from "outside unused files"
  disagreement counts.
- Fallow reports server actions in a separate `unused_server_actions` array.
  They are compared separately from its export/type arrays.
- Dependencies are compared as unused sets and unlisted sets. Depcheck has no
  separate unlisted array; its `missing` map is compared to Shrike/fallow
  unlisted.
- React absent props are compared as `(component_name, prop_name)` pairs. They
  are warning-only under D-047 and are not deletion candidates.
- Every outside-unused-file disagreement category was inspected in source.
  Representative members are listed in §6.

These are agreement counts, not precision/recall. A tool can be right and still
disagree because it reports a different category (de-export vs deletion), or
because it suppresses a whole file. The inspected disagreements are the
evidence; the counts are the map.

---

## 4. Performance

Format: **median wall seconds / median peak RSS MB**.

### 4.1 Shrike and fallow

| Repository | Shrike CLI cold | Shrike CLI warm | Shrike engine cold | Shrike engine warm | Fallow cold | Fallow warm |
|---|---:|---:|---:|---:|---:|---:|
| portfolio (22 files) | 0.233 / 57.7 | 0.210 / 57.7 | 0.056 / 14.6 | 0.015 / 14.6 | 0.517 / 109.1 | 0.444 / 109.1 |
| next-portfolio (31) | 0.255 / 57.9 | 0.208 / 57.8 | 0.028 / 14.6 | 0.014 / 14.6 | 0.457 / 109.3 | 0.424 / 109.2 |
| markalvarez (36) | 0.222 / 58.0 | 0.205 / 58.1 | 0.029 / 14.6 | 0.016 / 14.8 | 0.429 / 109.2 | 0.410 / 109.2 |
| requo (1,673) | 1.773 / 157.7 | 0.910 / 122.5 | 1.505 / 158.3 | 0.417 / 63.7 | 1.979 / 162.3 | 1.351 / 184.5 |
| accounteer (675) | 0.796 / 85.2 | 0.493 / 85.0 | 0.464 / 66.4 | 0.166 / 27.0 | 0.936 / 109.1 | 0.773 / 109.2 |

The earlier suite measured the pre-optimization Shrike CLI at 8.876 s cold and
3.151 s warm on requo, and the engine at 7.111 s cold and 1.059 s warm. The
final suite is a different run window, but the direction is unambiguous: the
CLI cold scan fell to 1.773 s, the warm scan to 0.910 s, and the engine to
1.505 s cold and 0.417 s warm. Isolated engine runs in a quieter window measured
1.12–1.16 s cold, with `discover=61 ms`, `scan=69–73 ms`, `parse=148–168 ms`,
`build=706–736 ms`, and `cache_write=80–166 ms`. The engine's own output is
byte-identical before and after the optimization (SHA-256 `fca3d3c5…` on
requo).

### 4.2 Knip and depcheck

| Repository | Knip default | Knip `--cache` | Depcheck |
|---|---:|---:|---:|
| portfolio | 2.917 / 203.7 | 3.351 / 183.3 | 2.654 / 205.5 |
| next-portfolio | 2.684 / 193.8 | 2.568 / 172.2 | 2.317 / 191.3 |
| markalvarez | 2.390 / 175.5 | 2.201 / 170.1 | 2.145 / 188.9 |
| requo | 15.146 / 676.9 | 9.815 / 611.5 | 26.098 / 447.5 |
| accounteer | 9.679 / 520.4 | 6.382 / 417.3 | 13.193 / 454.7 |

### 4.3 Ratios

Each cell is **competitor time ÷ Shrike time**. A value above 1 means Shrike is
faster; below 1 means the competitor is faster. The last two columns compare
fallow's cold and warm runs with the Shrike engine, so a value above 1 means the
engine is faster.

| Repository | Fallow cold / Shrike CLI cold | Fallow warm / Shrike CLI warm | Knip / Shrike CLI cold | Knip cache / Shrike CLI warm | Depcheck / Shrike CLI cold | Fallow cold / Engine cold | Fallow warm / Engine warm |
|---|---:|---:|---:|---:|---:|---:|---:|
| portfolio | 2.22 | 2.12 | 12.54 | 15.98 | 11.41 | 9.27 | 30.30 |
| next-portfolio | 1.80 | 2.04 | 10.53 | 12.34 | 9.09 | 16.39 | 30.48 |
| markalvarez | 1.93 | 2.00 | 10.77 | 10.76 | 9.67 | 14.57 | 26.21 |
| requo | 1.12 | 1.48 | 8.54 | 10.79 | 14.72 | 1.32 | 3.24 |
| accounteer | 1.18 | 1.57 | 12.16 | 12.96 | 16.57 | 2.02 | 4.66 |

### 4.4 What the numbers mean

- **Small repositories are startup-bound.** The Shrike CLI's 0.22–0.26 s on a
  22–36-file repository is mostly Node startup; the engine itself takes
  0.028–0.056 s cold and 0.014–0.016 s warm. The CLI still beats fallow cold by
  1.80–2.22× and warm by 2.00–2.12× because only the `scan` command module is
  loaded, and the engine is 9.3–16.4× faster cold and 26.2–30.5× faster warm.
- **Shrike now wins large cold scans.** On requo the CLI takes 1.773 s versus
  fallow's 1.979 s (1.12×); on accounteer 0.796 s versus 0.936 s (1.18×). The
  engine's margin is wider: 1.505 s versus 2.063 s (1.32×) on requo and 0.464 s
  versus 0.934 s (2.02×) on accounteer. Isolated engine runs in a quieter window
  measured 1.12–1.16 s cold. The engine still does more work (a full symbol graph
  with references and entries, plus the member and component checks); the cold
  path is now dominated by parsing and graph construction, not discovery or
  redundant serialization.
- **Shrike wins warm by a larger margin.** On requo the CLI is 0.910 s versus
  fallow's 1.351 s (1.48×); on accounteer 0.493 s versus 0.773 s (1.57×). The
  engine is 3.24× and 4.66× faster warm. The remaining CLI overhead over the
  engine is the dependency second pass plus writing the 24 MB JSON output; it
  is no longer dominated by decoding or canonicalizing the snapshot.
- **The output path is no longer the bottleneck.** The engine returns a compact
  `dependencyInput` projection alongside the already-serialized snapshot. The
  CLI validates that projection, runs the dependency pass from it, and splices
  the engine's snapshot bytes into the final document instead of parsing and
  re-encoding 24 MB. On requo the CLI's warm overhead over the engine is ~0.49 s
  (0.910 s vs 0.417 s), and peak RSS fell from 339 MB to 122 MB.
- **Memory.** The engine is the lightest tool (15 MB on the small apps, 158 MB
  cold / 64 MB warm on requo). The Shrike CLI is 58 MB on the small apps,
  158 MB cold / 122 MB warm on requo, and 85 MB on accounteer; fallow is
  109–184 MB, knip 170–677 MB, depcheck 189–455 MB.
- **Cold-to-warm gains.** Shrike CLI requo 1.773 → 0.910 s (1.9×); engine 1.505
  → 0.417 s (3.6×). Fallow requo 1.979 → 1.351 s (1.5×). Knip requo 15.15 →
  9.82 s (1.5×). Small repositories gain little because startup dominates.

### 4.5 Optimization summary

The work stayed inside the existing architecture: the engine still owns
resolution, entry detection, and graph analysis, and the CLI still owns the
dependency second pass. No analysis rule changed.

Engine:

- **Compiled discovery filters.** `PathFilter` compiles the include/ignore globs
  once with `globset` and falls back to the reference matcher for unsupported
  syntax, non-ASCII paths or patterns, and semantic edge cases. Discovery on
  requo fell from ~168 ms to ~63–72 ms with byte-identical output.
- **Concurrent finish passes.** The independent member, component-prop,
  internal-only, and SCC passes run under `rayon::join` and append diagnostics in
  the original pass order. The inner `prop_blocked`/`prop_uses` loops are also
  parallel.
- **Parallel edge sort and hash-set reachability.** The per-pass edge sort uses
  `par_sort_by` with the same total order, so the output is unchanged. The
  reachability BFS uses hash sets instead of sorted-vector membership and avoids
  cloning the entry list. Together these cut the finish stage on requo without
  changing a single diagnostic.
- **No-op early exits.** A repository with no enums skips enum member analysis
  entirely (~124 ms on requo).
- **Compact dependency projection.** The analyze response includes a
  `dependencyInput` view (files, imports, exports) so the CLI does not decode
  the full 24 MB snapshot to run the dependency pass.
- **Serialized snapshot reuse.** The engine serializes the snapshot once and
  returns the bytes; the CLI writes those bytes directly.

CLI:

- **Lazy command loading.** `main.ts` imports only the selected command, and
  `config.ts` loads `@shrike/verifier` only when an executable config exists.
  This cut small-repo startup from ~0.59 s to ~0.23 s.
- **Buffer output.** The engine response is kept as a `Buffer`; `writeRawJson`
  writes the snapshot chunk directly to stdout with no full-response UTF-8
  decode or large string copy.
- **Dependency-pass prefilters.** A literal `text.includes(name)` check runs
  before the regex, directory listings and alias sets are cached, and redundant
  `existsSync` calls are gone.

Accuracy gates: the engine output is byte-identical to the pre-optimization
binary on all five repositories (`fca3d3c5…` on requo), the CLI's parsed JSON is
identical on all five, 337 Rust tests and 134 Node tests pass, and the corpus,
differential, and fixture gates pass unchanged.

---

## 5. Accuracy

### 5.1 Unused files

| Repository | Shrike | Knip | Fallow | All three | Notes |
|---|---:|---:|---:|---:|---|
| portfolio | 0 | 0 | 0 | 0 | — |
| next-portfolio | 3 | 3 | 3 | 3 | all three agree |
| markalvarez | 3 | 3 | 3 | 3 | all three agree |
| requo | 17 | 18 | 17 | 17 | knip-only `config/drizzle.config.ts` is a false positive |
| accounteer | 11 | 12 | 10 | 10 | Shrike-only `tests/fixtures/resumes.ts` is genuine; knip-only `tests/stubs/server-only.ts` is a false positive |

Inspected file disagreements:

- **requo `config/drizzle.config.ts` (knip-only).** `package.json` scripts pass
  `--config=config/drizzle.config.ts` to `drizzle-kit`; knip does not parse CLI
  arguments, so it reports the file as unused. Shrike and fallow keep it live.
- **accounteer `tests/fixtures/resumes.ts` (Shrike-only).** No source or test
  imports it. Shrike is right; knip also reports it, fallow misses it.
- **accounteer `tests/stubs/server-only.ts` (knip-only).** `vitest.config.ts`
  aliases `server-only` to this file, so it is a test-runtime module. Knip does
  not see the alias; Shrike and fallow keep it live.

### 5.2 Unused symbols

"Combined" is Shrike exported candidates + `internalOnly`, knip exports +
types, fallow exports + types. "Outside" excludes symbols in files that any tool
reported unused.

| Repository | Shrike exported | Shrike internalOnly | Shrike combined | Knip combined | Fallow combined | Three-way overlap | Shrike-only outside | Knip-only outside | Fallow-only outside |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| portfolio | 1 | 5 | 6 | 6 | 1 | 1 | 0 | 0 | 0 |
| next-portfolio | 8 | 0 | 8 | 0 | 0 | 0 | 0 | 0 | 0 |
| markalvarez | 23 | 2 | 25 | 2 | 1 | 1 | 0 | 0 | 0 |
| requo | 595 | 673 | 1,268 | 1,190 | 879 | 839 | 24 | 13 | 0 |
| accounteer | 162 | 447 | 609 | 545 | 286 | 251 | 26 | 12 | 0 |

Shrike-only symbols **inside** unused files, which are not disagreements because
knip/fallow suppress whole files: next-portfolio 8, markalvarez 23, requo 27,
accounteer 15.

The 24/26 Shrike-only outside-unused-file symbols break down as follows.

**requo (24)**

- 2 exported deletion candidates:
  - `lib/ai/model-options.ts#getModelsForProvider` — the inner function is used
    only by `lib/ai/config.ts#getModelsForProvider`, which all three tools report
    as dead. Knip and fallow see the import from the dead wrapper and keep the
    inner function live; Shrike propagates deadness transitively and reports it.
  - `lib/ai/types.ts#AiRoutingProfile` — a re-export alias used only by
    `lib/ai/index.ts#AiRoutingProfile`, which all three tools report as dead.
    Again, Shrike reports the inner alias; knip/fallow do not.
- 1 exported de-export candidate: `lib/deployment.ts#DeploymentMode` is
  re-exported from `lib/deployment-mode.ts`; consumers import
  `parseDeploymentMode` from that module directly, so the re-export has no
  external importer. The type is used inside `lib/deployment.ts`, so this is an
  unnecessary export, not a dead declaration.
- 21 advisory `internalOnly` entries:
  - 5 `RESOLVED_*` constants in `lib/ai/embeddings.ts` whose `EMBEDDING_*`
    aliases are separately reported as dead;
  - 13 exports in `scripts/*.ts` (for example `scripts/audit-color-system.ts#Rgb`
    and `scripts/audit-tenant-boundary.ts#collectViolations`). Scripts are module
    entries under D-021, so the file is live but the exports are unnecessary;
  - 3 deployment types (`BillingDriver`, `BillingProvider`,
    `EntitlementPolicy`) used only inside `lib/deployment.ts`.

The two `lib/deployment.ts` functions that fallow reports (`isCloudDeployment`
and `isSelfHostedDeployment`) are now shared: Shrike lists them as `internalOnly`
and fallow lists them as unused exports, so they are not in the 24.

**accounteer (26)**

- 13 server actions in `lib/domain/candidates/actions.ts` (2) and
  `lib/domain/employers/actions.ts` (11). All 13 are also in fallow's
  `unused_server_actions`; knip misses them. They are not Shrike-only relative
  to fallow's server-action rule.
- 3 exported candidates in `lib/analytics/client.ts`:
  `clientAnalyticsConfigured` and `currentConsentState` are declarations with no
  reference outside the module, and `CONSENT_REOPEN_EVENT` is an unused
  re-export; consumers import it from `lib/analytics/consent.ts`, not from the
  client module. Knip and fallow miss all three.
- 10 advisory `internalOnly` entries: `SetupSearchParams`,
  `TrackClientOptions`, `TrackClientOutcome`, `anonymousId`, `CompanyClaim`,
  `FoundingGrant`, `JobGraphResult`, `MemberWorkEmail`, `VerificationCase`, and
  `ndcgAt`. These are de-export-only; none is a deletion candidate.

Knip-only outside-unused-file symbols:

- **requo (13).** Twelve field-name types plus `CspMode`. They are used in
  `Record<...>` or in function signatures in their own file, but have no external
  importer. Knip calls them unused types; Shrike keeps them live through
  type-shape visibility. They are de-export candidates, not dead declarations.
  Examples: `features/quotes/types.ts#QuoteEditorFieldName` is used at line 412
  in `Record<QuoteEditorFieldName, string[] | undefined>`;
  `lib/security/csp.ts#CspMode` is used at line 191 as a parameter type.
- **accounteer (12).** Eleven type/usage differences and `TxHandle`.
  - The 11 types are used in their own file's exported interfaces or function
    signatures; Shrike keeps them live, knip reports the exports as unused.
  - `lib/domain/inbox/repository.ts#TxHandle` is imported through an inline
    `import("@/lib/domain/inbox/repository").TxHandle`; knip reports it as an
    unused type. This is a knip false positive.
  - The former `lib/auth/workspace-actions.ts#createEmployerWorkspaceAction`
    difference is now shared: the assertion narrowing exposed it, and both
    Shrike and knip report it.

Fallow-only outside-unused-file symbols: none. The two previous requo
differences (`isCloudDeployment` and `isSelfHostedDeployment`) are now shared
with Shrike as de-export candidates rather than disagreements.

### 5.3 Server actions

| Repository | Shrike | Fallow | Knip | Agreement |
|---|---:|---:|---:|---|
| requo | 25 | 25 | 25 | Shrike ∩ fallow = 25/25; knip = 25/25 |
| accounteer | 23 | 23 | 10 | Shrike ∩ fallow = 23/23; knip misses 13 |

Shrike's graph-based `serverActionPolicy: "graph"` (D-020) matches fallow's
server-action rule exactly on both repositories. Knip's accounteer coverage is
incomplete: it misses the 13 actions listed in §5.2. Shrike also reports
unreferenced schemas and helpers in `"use server"` modules (for example
`requestApprovalSchema` and `decideChangeOrderSchema` on requo); those are not
server actions and are not included in the 25/23 counts.

### 5.4 Dependencies

| Repository | Shrike unused | Knip unused | Fallow unused | Depcheck unused | Shrike unlisted | Knip unlisted | Fallow unlisted |
|---|---:|---:|---:|---:|---:|---:|---:|
| portfolio | 0 | 1 | 1 | 4 | 0 | 0 | 0 |
| next-portfolio | 2 | 2 | 0 | 3 | 0 | 0 | 0 |
| markalvarez | 0 | 1 | 1 | 5 | 0 | 0 | 0 |
| requo | 3 | 3 | 3 | 5 | 3 | 0 | 2 |
| accounteer | 0 | 0 | 0 | 2 | 1 | 0 | 1 |

**Unused dependencies**

- Shrike's unused set is an exact match with knip on next-portfolio
  (`@base-ui/react`, `class-variance-authority`) and requo (`@next/playwright`,
  `fast-check`, `ngrok`). The next-portfolio packages are used only by files all
  three tools report unused.
- Shrike reports no unused dependencies on portfolio, markalvarez, or
  accounteer.
- Knip's extras:
  - portfolio: `@tailwindcss/turbopack`;
  - markalvarez: `@tailwindcss/turbopack`.
- Fallow's extra: `@tailwindcss/turbopack` on portfolio and markalvarez.
- `@tailwindcss/turbopack` is referenced in `next.config.ts` as a loader, so it
  is config-only, not dead. These are false positives for deletion.
- Depcheck's extra findings beyond the shared set are config-only packages
  (`tailwindcss`, `@tailwindcss/postcss`, `@types/node`, `@types/react-dom`,
  `tw-animate-css`) and vary by repository.

**Unlisted dependencies**

- requo: Shrike finds `playwright`, `playwright-core`, and `uuid`; fallow finds
  `playwright-core` and `uuid`; knip finds none. All three are imported and
  undeclared:
  - `playwright` in `scripts/check-hydration.mjs` and
    `scripts/analytics-screenshots.mjs`;
  - `playwright-core` in `scripts/marketing/*.mjs`;
  - `uuid` in `lib/ids.ts`.
- accounteer: Shrike and fallow find `@better-auth/utils`; knip finds none. It is
  imported in `db/seeds/seed.ts` and undeclared.
- Depcheck reports `uuid` as missing on requo, agreeing with Shrike, but it does
  not model unlisted dependencies as a separate set.

### 5.5 React absent props (warning-only)

These are `absent_component_prop` warnings under D-047: React-gated, optional
props actually read, abstaining on escapes, namespace uses, spreads, public/
entry/generated/dead components, and no-site cases. They are **not deletion
candidates**.

| Repository | Shrike | Fallow | Overlap | Notes |
|---|---:|---:|---:|---|
| portfolio | 33 | 33 | 33 | exact agreement: 19 `HalftoneReveal` + 14 `Folder` props |
| next-portfolio | 0 | 0 | 0 | — |
| markalvarez | 2 | 0 | 0 | `SubPage.className`, `ThemeToggle.className`; spot-checked as true positives |
| requo | 197 | 0 | 0 | warning-only; fallow recall difference |
| accounteer | 80 | 0 | 0 | warning-only; fallow recall difference |

The portfolio result is exact agreement: both tools report the same 33 optional
props — 19 on named `HalftoneReveal` and 14 on default-exported `const Folder` in
`app/components/folder.tsx`. The single render site at
`app/components/project-folder.tsx:112` supplies 15 props and omits `bounce`,
`children`, `className`, `closeOnOutsideClick`, `defaultOpen`, `frost`, `layout`,
`onItemDrop`, `onOpenChange`, `open`, `peek`, `style`, `tabPosition`, and `tilt`.
The default-exported `const` case is the one that used to be a Shrike miss; the
D-047 follow-up maps the local binding under both `#Folder` and `#default`, so it
is now reported.

The markalvarez findings were spot-checked: `ThemeToggle` has two render sites,
both `<ThemeToggle />`, and `className` is read at line 58; `SubPage` has six
render sites, none passes `className`, and it is read in `cn(...)`. Fallow
reports neither, so this is a fallow recall difference.

The requo/accounteer counts are much larger than fallow's. A synthetic probe
showed fallow supports inline and exported props types, so this is not a parser
limitation; it is a reachability/scale difference in fallow's rule. I did not
manually verify all 277 warnings; they are warning-only and are excluded from
the deletion-candidate precision claims.

---

## 6. Inspected disagreements

This section is the accuracy evidence. Each row is a source inspection, not an
inferred classification.

| Repository | Finding | Inspection | Classification |
|---|---|---|---|
| requo | `lib/ai/model-options.ts#getModelsForProvider` Shrike-only | `lib/ai/config.ts` imports it and wraps it; the wrapper is reported dead by all three tools; no other importer exists | Shrike true positive; knip/fallow transitive-deadness miss |
| requo | `lib/ai/types.ts#AiRoutingProfile` Shrike-only | Re-export alias used only by `lib/ai/index.ts#AiRoutingProfile`, which all three tools report dead; consumers import `@/lib/ai/routing-profiles` directly | Shrike true positive; knip/fallow transitive-deadness miss |
| requo | `lib/deployment.ts#DeploymentMode` Shrike-only | Re-exported from `lib/deployment-mode.ts`; consumers import `parseDeploymentMode` from that module directly; the type is used inside `lib/deployment.ts` | Shrike true positive de-export candidate; knip/fallow miss |
| requo | 13 knip-only types | Used in `Record<...>` or function signatures in their own file; no external importer | De-export candidates; Shrike keeps them live through type-shape visibility |
| accounteer | `TxHandle` knip-only | Imported through inline `import("@/lib/domain/inbox/repository").TxHandle` | Knip false positive |
| accounteer | `clientAnalyticsConfigured`, `currentConsentState`, `CONSENT_REOPEN_EVENT` Shrike-only | No reference to the first two outside `lib/analytics/client.ts`; consumers import `CONSENT_REOPEN_EVENT` from `lib/analytics/consent.ts`, so the client re-export is unused | Shrike true positives; knip/fallow miss |
| accounteer | `tests/fixtures/resumes.ts` Shrike-only file | No importer in source or tests | Shrike true positive; fallow miss, knip agrees |
| requo | `config/drizzle.config.ts` knip-only file | `package.json` scripts pass `--config=config/drizzle.config.ts` to `drizzle-kit` | Knip false positive; config-only |
| accounteer | `tests/stubs/server-only.ts` knip-only file | `vitest.config.ts` aliases `server-only` to this stub | Knip false positive; test-runtime module |
| portfolio | `react-dom`, `@types/react-dom` knip-only deps | Next.js runtime/type packages, not imported directly | Knip false positives for deletion |
| portfolio, markalvarez | `@tailwindcss/turbopack` knip/fallow-only dep | Referenced as a loader in `next.config.ts` | False positive; config-only |
| markalvarez | `SubPage.className`, `ThemeToggle.className` Shrike-only | Optional props read and omitted at all render sites | Shrike true positives; fallow recall difference |

No inspected outside-unused-file disagreement was a Shrike false positive on a
deletion candidate. Fallow-only differences are now zero on both large
repositories, and the default-exported-const absent-prop gap is closed. The
remaining Shrike abstentions are opaque namespace dynamic imports; returned
loaders and property assertions are narrowed (D-048/D-049).

---

## 7. Caveats and threats to validity

1. **No complete ground truth.** The counts are agreement sets, not precision
   and recall. The evidence is the inspected disagreement set, not a labeled
   corpus of every symbol in a 1,673-file application.
2. **Different categories.** Shrike's `candidates` are deletion candidates;
   `internalOnly` is advisory. Knip/fallow's "unused exports" include de-export
   candidates. A knip-only or fallow-only finding is not automatically a
   competitor win.
3. **Suppression differences.** Knip and fallow suppress exports inside files
   they report unused. Shrike reports symbols individually. Inside-unused-file
   symbols are reported separately and excluded from disagreement counts.
4. **Fallow's absent-prop rule is narrower.** It reports 0 on requo/accounteer
   despite supporting inline and exported prop types in a synthetic probe. The
   reason is a reachability/scale difference, not a parser limitation. Shrike's
   197/80 warnings are not all manually verified.
5. **Cache semantics differ.** Shrike caches analysis summaries; fallow caches
   its own analysis; knip's cache is opt-in. Cold/warm comparisons are
   like-for-like in intent, not identical implementations.
6. **Shared machine, WSL2, and no CPU pinning.** Timing is noisier than bare
   metal. The final table is the paired suite run at 23:12–23:15 with load
   average ~3–6 from unrelated work. Each round runs both tools in alternating
   order, so load drift affects both; absolute times are higher than an idle
   suite, but the ratios are paired. Isolated engine runs in a quieter window
   measured 1.12–1.16 s cold on requo. The earlier 22:44, 19:12 idle, and 17:xx
   Playwright-overlapped suites are preserved under `/tmp/opencode/bench/` but
   are not used for the final table.
7. **Five rounds, medians only.** Five measured rounds per configuration,
   medians reported, no p95. The three small repositories are dominated by
   process startup. Knip and depcheck were measured in the earlier suite and
   were not re-run in the paired suite.
8. **Copies, not commits.** The benchmark copies have no `.git`; commit pinning
   is unavailable. Next.js versions and package names are recorded. The
   `node_modules` directories are symlinked from the working copies.
9. **No `next build` in the benchmark.** The comparison measures static
   analysis. Build-evidence suppression was validated separately by the corpus
   and fixture gates; it is not re-run for every finding here.
10. **Shrike working tree.** The release binary and CLI were built from the
    uncommitted working tree at `35040c6`, including the performance work
    (D-051) and the three benchmark-driven accuracy fixes (D-047 follow-up,
    D-048, D-049). The report is reproducible only with that tree, not from the
    commit alone. The engine output is byte-identical to the pre-optimization
    engine, and the CLI's parsed JSON is identical on all five repositories.
11. **Depcheck noise.** Depcheck exits 255 with findings, has no cache, and
    flags config-only packages. Its unused set is not directly comparable to
    knip/fallow without filtering.
12. **Server actions are separate.** Fallow's `unused_server_actions` is not part
    of its export/type arrays. The combined symbol table therefore shows
    accounteer's 13 actions as Shrike-only even though fallow agrees with them in
    its separate rule.
13. **Remaining conservative abstentions.** The fixes close the
    default-exported-const absent-prop gap and narrow returned loaders and
    assertion-only namespace bindings. Opaque namespace uses (a bare, exported,
    or computed dynamic-import namespace, or a loader with any other reference)
    still keep a whole module's export surface live. Those are deliberate
    abstentions, not claims that the symbols are dead.

---

## 8. Reproduction

```bash
# Shrike release engine and CLI
cargo build --release -p shrike-engine
pnpm --filter shrike build

# Paired, interleaved suite (Shrike vs fallow, all five repositories)
# The final medians are saved in
# /tmp/opencode/bench/paired-optimized-all-summary.json
python3 /tmp/opencode/bench/paired.py

# Time one configuration (example: requo, Shrike CLI, cold)
python3 /tmp/opencode/bench/timeit.py \
  --runs 5 --warmup 0 \
  --cleanup /tmp/opencode/bench-repos/requo/.shrike/cache \
  --cwd /tmp/opencode/bench-repos/requo \
  --out /tmp/opencode/bench/shrike-requo.json \
  --err /tmp/opencode/bench/shrike-requo.err \
  --env SHRIKE_ENGINE=/home/mark/code/projects/shrike/target/release/shrike-engine \
  -- node /home/mark/code/projects/shrike/packages/cli/dist/main.js scan --json

# Accuracy comparison
python3 /tmp/opencode/bench/compare.py

# Old-vs-new engine byte comparison and CLI parsed-JSON comparison
python3 /tmp/opencode/bench/engine-diff.py
python3 /tmp/opencode/bench/cli-diff.py
```

The harness and comparison scripts are:

- `/tmp/opencode/bench/paired.py` — paired wall time and peak RSS;
- `/tmp/opencode/bench/timeit.py` — single-command wall time and peak RSS;
- `/tmp/opencode/bench/compare.py` — normalized `file#name` comparison;
- `/tmp/opencode/bench/engine-diff.py` — old-vs-new engine bytes;
- `/tmp/opencode/bench/cli-diff.py` — old-vs-new parsed CLI JSON.

The final paired summary is
`/tmp/opencode/bench/paired-all-summary.json`; per-run outputs are under
`/tmp/opencode/bench/paired/`.

Raw artifacts:

- timing: `/tmp/opencode/bench/time-*.json`;
- Shrike CLI: `/tmp/opencode/bench/shrike-*.json`;
- Shrike engine: `/tmp/opencode/bench/engine-*.json`;
- knip: `/tmp/opencode/bench/knip-*.raw`;
- fallow: `/tmp/opencode/bench/fallow-*.json` and
  `/tmp/opencode/bench/fallow-*-props.json`;
- depcheck: `/tmp/opencode/bench/depcheck-*.raw`;
- fixtures: `/tmp/opencode/absent-default/` and
  `/tmp/opencode/fallow-absent-probe2/`;
- repository copies: `/tmp/opencode/bench-repos/`;
- tools: `/tmp/opencode/bench-tools/node_modules/`.

These artifacts are local and are not committed because the raw Shrike JSON is
tens of megabytes per large repository. The commands above regenerate them.
