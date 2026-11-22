# Shrike competitor accuracy benchmark — labeled corpus

**Date:** 2026-11-22T17:35:04.301686+00:00  
**Corpus:** 315 cases / 732 labeled items from `fixtures/must-not-report`  
**Tools:** shrike-cli, fallow, knip, ts-prune, ts-unused-exports  
**Machine:** AMD Ryzen 7 5700U with Radeon Graphics, 8 logical CPUs  
**Shrike:** 35040c69f2c805f4cdc27c4f014cc06ef6049f68 (working tree)  

---

## 1. What this measures

The performance benchmark measures speed. This benchmark measures whether a finding is correct on cases where the answer is known before any tool runs. Each case is a minimal project with framework-derived labels: protected symbols and files that must never be reported as dead, and must-report symbols and files that must be found. The labels come from framework documentation and a two-sided fixture review, not from any analyzer's output.

Scoring is at the labeled item level:

```text
TP = mustReport symbols found + unusedFiles reported
FP = protected symbols reported + usedFiles reported
FN = mustReport symbols missed + unusedFiles missed
precision = TP / (TP + FP)   recall = TP / (TP + FN)
```

A symbol counts as found when the tool reports it or reports its whole file unused. `usedFiles` reported unused is scored with a closure allowance: if every importer of the file is itself reported unused, the file is transitively dead and is not a false positive. The strict count is shown separately. The narrow tools (ts-prune, ts-unused-exports) only see exports, so file-level labels are excluded from their metrics. Member-level warnings (unused enum/class/namespace members) are not scored; they are not deletions of the enclosing export.

---

## 2. Headline

| Tool | Scope | Valid cases | TP | FP | FN | Precision | Recall | F1 | Case pass |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| shrike-cli | symbols + files | 315 | 196 | 0 | 0 | 100.0% | 100.0% | 100.0% | 315/315 |
| fallow | symbols + files | 315 | 180 | 68 | 16 | 72.6% | 91.8% | 81.1% | 278/315 |
| knip | symbols + files | 314 | 143 | 82 | 47 | 63.6% | 75.3% | 68.9% | 257/314 |
| ts-prune | symbols | 315 | 94 | 183 | 96 | 33.9% | 49.5% | 40.3% | 131/315 |
| ts-unused-exports | symbols | 283 | 102 | 160 | 18 | 38.9% | 85.0% | 53.4% | 167/283 |

Precision uses the closure-adjusted file score. Strict precision (counting every reported `usedFiles` entry as a false positive) is in §3.

---

## 3. Strict file scoring and unlabeled findings

| Tool | TP | FP (closure) | FP (strict) | Precision (closure) | Precision (strict) | Extra symbols | Extra files | Member warnings |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| shrike-cli | 196 | 0 | 0 | 100.0% | 100.0% | 2 | 67 | 0 |
| fallow | 180 | 68 | 69 | 72.6% | 72.3% | 5 | 124 | 23 |
| knip | 143 | 82 | 83 | 63.6% | 63.3% | 6 | 131 | 24 |
| ts-prune | 94 | 183 | 183 | 33.9% | 33.9% | 289 | 0 | 0 |
| ts-unused-exports | 102 | 160 | 160 | 38.9% | 38.9% | 289 | 0 | 0 |

Extra findings are reported items that are neither protected nor must-report. They are not counted in precision because the corpus does not label them. Member warnings are unused enum/class/namespace members, which are not deletions of the enclosing export.

---

## 4. By category

| Category | Cases | shrike-cli P / R / F1 | fallow P / R / F1 | knip P / R / F1 | ts-prune P / R / F1 | ts-unused-exports P / R / F1 |
|---|---:|---:|---:|---:|---:|---:|
| astro | 1 | 100.0% / 100.0% / 100.0% | 100.0% / 100.0% / 100.0% | — / 0.0% / — | — / 0.0% / — | — / 0.0% / — |
| barrel | 15 | 100.0% / 100.0% / 100.0% | 100.0% / 87.5% / 93.3% | 100.0% / 100.0% / 100.0% | 100.0% / 50.0% / 66.7% | 100.0% / 50.0% / 66.7% |
| config-entry | 8 | — / — / — | — / — / — | 0.0% / — / — | — / — / — | — / — / — |
| cycle | 8 | — / — / — | — / — / — | — / — / — | — / — / — | — / — / — |
| default-reexport | 13 | — / — / — | — / — / — | — / — / — | 0.0% / — / — | — / — / — |
| docusaurus | 1 | 100.0% / 100.0% / 100.0% | 100.0% / 100.0% / 100.0% | 57.1% / 100.0% / 72.7% | 57.1% / 100.0% / 72.7% | 57.1% / 100.0% / 72.7% |
| dynamic-import | 18 | 100.0% / 100.0% / 100.0% | 66.7% / 100.0% / 80.0% | 85.7% / 100.0% / 92.3% | — / 0.0% / — | 12.5% / 50.0% / 20.0% |
| expo | 4 | 100.0% / 100.0% / 100.0% | 58.8% / 100.0% / 74.1% | 100.0% / 40.0% / 57.1% | — / 0.0% / — | — / 0.0% / — |
| export-all | 8 | — / — / — | — / — / — | — / — / — | — / — / — | — / — / — |
| gatsby | 1 | 100.0% / 100.0% / 100.0% | 100.0% / 66.7% / 80.0% | 100.0% / 33.3% / 50.0% | 60.0% / 100.0% / 75.0% | 60.0% / 100.0% / 75.0% |
| generated-clients | 10 | — / — / — | — / — / — | — / — / — | — / — / — | — / — / — |
| intercepting-routes | 20 | — / — / — | — / — / — | — / — / — | 0.0% / — / — | 0.0% / — / — |
| internal-only | 4 | 100.0% / 100.0% / 100.0% | 100.0% / 100.0% / 100.0% | 83.3% / 100.0% / 90.9% | 66.7% / 40.0% / 50.0% | 66.7% / 40.0% / 50.0% |
| jsx | 8 | — / — / — | — / — / — | — / — / — | 0.0% / — / — | — / — / — |
| ladle | 1 | 100.0% / 100.0% / 100.0% | 25.0% / 66.7% / 36.4% | 100.0% / 100.0% / 100.0% | 33.3% / 66.7% / 44.4% | 33.3% / 66.7% / 44.4% |
| mdx | 2 | 100.0% / 100.0% / 100.0% | 80.0% / 80.0% / 80.0% | 80.0% / 80.0% / 80.0% | 71.4% / 100.0% / 83.3% | 71.4% / 100.0% / 83.3% |
| metro | 1 | 100.0% / 100.0% / 100.0% | 100.0% / 100.0% / 100.0% | 41.7% / 100.0% / 58.8% | 55.6% / 100.0% / 71.4% | 55.6% / 100.0% / 71.4% |
| monorepo-client-boundary | 20 | 100.0% / 100.0% / 100.0% | 100.0% / 100.0% / 100.0% | 100.0% / 100.0% / 100.0% | 50.0% / 100.0% / 66.7% | 100.0% / 100.0% / 100.0% |
| monorepo-next-paths | 1 | 100.0% / 100.0% / 100.0% | 100.0% / 100.0% / 100.0% | 100.0% / 85.7% / 92.3% | 87.5% / 100.0% / 93.3% | 87.5% / 100.0% / 93.3% |
| namespace | 14 | 100.0% / 100.0% / 100.0% | 100.0% / 100.0% / 100.0% | 100.0% / 100.0% / 100.0% | 100.0% / 100.0% / 100.0% | 100.0% / 100.0% / 100.0% |
| next-config | 3 | 100.0% / 100.0% / 100.0% | 50.0% / 100.0% / 66.7% | 60.0% / 60.0% / 60.0% | — / 0.0% / — | — / 0.0% / — |
| next-image-metadata | 1 | 100.0% / 100.0% / 100.0% | 66.7% / 100.0% / 80.0% | — / 0.0% / — | — / 0.0% / — | — / 0.0% / — |
| next-instrumentation | 1 | 100.0% / 100.0% / 100.0% | 100.0% / 100.0% / 100.0% | 100.0% / 50.0% / 66.7% | 50.0% / 100.0% / 66.7% | 50.0% / 100.0% / 66.7% |
| next-interrupt | 1 | 100.0% / 100.0% / 100.0% | 100.0% / 100.0% / 100.0% | — / 0.0% / — | — / 0.0% / — | — / 0.0% / — |
| next-page-data | 1 | 100.0% / 100.0% / 100.0% | — / 0.0% / — | — / 0.0% / — | 0.0% / 0.0% / — | 0.0% / 0.0% / — |
| next-pages | 3 | 100.0% / 100.0% / 100.0% | 71.4% / 62.5% / 66.7% | 66.7% / 50.0% / 57.1% | 83.3% / 62.5% / 71.4% | 83.3% / 62.5% / 71.4% |
| next-segment-config | 2 | 100.0% / 100.0% / 100.0% | 58.8% / 100.0% / 74.1% | 100.0% / 20.0% / 33.3% | — / 0.0% / — | — / 0.0% / — |
| nuxt | 1 | 100.0% / 100.0% / 100.0% | 100.0% / 100.0% / 100.0% | 100.0% / 100.0% / 100.0% | — / 0.0% / — | — / 0.0% / — |
| package-entry | 8 | — / — / — | — / — / — | — / — / — | 0.0% / — / — | 0.0% / — / — |
| page-entry | 8 | — / — / — | — / — / — | — / — / — | 0.0% / — / — | 0.0% / — / — |
| pages-router-route-conflict | 1 | 100.0% / 100.0% / 100.0% | 100.0% / 100.0% / 100.0% | — / 0.0% / — | 33.3% / 100.0% / 50.0% | 33.3% / 100.0% / 50.0% |
| parallel-routes | 20 | — / — / — | — / — / — | — / — / — | 0.0% / — / — | 0.0% / — / — |
| react | 1 | 100.0% / 100.0% / 100.0% | 50.0% / 100.0% / 66.7% | 50.0% / 100.0% / 66.7% | — / 0.0% / — | — / 0.0% / — |
| react-component-loader | 1 | 100.0% / 100.0% / 100.0% | 100.0% / 100.0% / 100.0% | 60.0% / 60.0% / 60.0% | — / 0.0% / — | 50.0% / 100.0% / 66.7% |
| react-component-prop | 1 | 100.0% / 100.0% / 100.0% | — / 0.0% / — | — / 0.0% / — | — / 0.0% / — | — / 0.0% / — |
| react-cosmos | 2 | 100.0% / 100.0% / 100.0% | 51.9% / 100.0% / 68.3% | 64.7% / 78.6% / 71.0% | 57.9% / 78.6% / 66.7% | 57.9% / 78.6% / 66.7% |
| react-router | 2 | 100.0% / 100.0% / 100.0% | 77.8% / 100.0% / 87.5% | 9.1% / 14.3% / 11.1% | 9.1% / 14.3% / 11.1% | 9.1% / 14.3% / 11.1% |
| remix | 3 | 100.0% / 100.0% / 100.0% | 55.6% / 100.0% / 71.4% | 33.3% / 100.0% / 50.0% | 100.0% / 60.0% / 75.0% | 100.0% / 60.0% / 75.0% |
| route-groups | 20 | — / — / — | — / — / — | — / — / — | 0.0% / — / — | 0.0% / — / — |
| script-entry | 1 | 100.0% / 100.0% / 100.0% | 100.0% / 80.0% / 88.9% | 100.0% / 80.0% / 88.9% | — / 0.0% / — | — / 0.0% / — |
| server-action | 9 | 100.0% / 100.0% / 100.0% | 100.0% / 100.0% / 100.0% | 100.0% / 100.0% / 100.0% | 100.0% / 100.0% / 100.0% | 100.0% / 100.0% / 100.0% |
| side-effect | 8 | — / — / — | — / — / — | — / — / — | — / — / — | — / — / — |
| storybook | 2 | 100.0% / 100.0% / 100.0% | 100.0% / 100.0% / 100.0% | 83.3% / 100.0% / 90.9% | — / 0.0% / — | — / 0.0% / — |
| sveltekit | 1 | 100.0% / 100.0% / 100.0% | 100.0% / 100.0% / 100.0% | 50.0% / 100.0% / 66.7% | — / 0.0% / — | — / 0.0% / — |
| tanstack | 2 | 100.0% / 100.0% / 100.0% | 100.0% / 100.0% / 100.0% | 40.0% / 100.0% / 57.1% | — / 0.0% / — | — / 0.0% / — |
| test-entry | 10 | 100.0% / 100.0% / 100.0% | 29.4% / 100.0% / 45.5% | 29.4% / 100.0% / 45.5% | 25.0% / 25.0% / 25.0% | 25.0% / 25.0% / 25.0% |
| type-only | 8 | — / — / — | — / — / — | — / — / — | — / — / — | — / — / — |
| unused-class-member | 1 | 100.0% / 100.0% / 100.0% | 100.0% / 100.0% / 100.0% | 100.0% / 100.0% / 100.0% | 100.0% / 100.0% / 100.0% | 100.0% / 100.0% / 100.0% |
| unused-enum-member | 1 | 100.0% / 100.0% / 100.0% | 50.0% / 100.0% / 66.7% | 50.0% / 100.0% / 66.7% | 50.0% / 100.0% / 66.7% | 50.0% / 100.0% / 66.7% |
| unused-file | 1 | 100.0% / 100.0% / 100.0% | 100.0% / 100.0% / 100.0% | 100.0% / 100.0% / 100.0% | 66.7% / 66.7% / 66.7% | 66.7% / 66.7% / 66.7% |
| unused-namespace-member | 1 | 100.0% / 100.0% / 100.0% | — / 0.0% / — | — / 0.0% / — | 0.0% / 0.0% / — | 0.0% / 0.0% / — |
| use-cache | 20 | — / — / — | — / — / — | — / — / — | — / — / — | — / — / — |
| use-client | 8 | — / — / — | — / — / — | — / — / — | 0.0% / — / — | — / — / — |
| vite | 2 | 100.0% / 100.0% / 100.0% | 100.0% / 100.0% / 100.0% | 100.0% / 100.0% / 100.0% | — / 0.0% / — | — / 0.0% / — |
| webpack | 1 | 100.0% / 100.0% / 100.0% | 100.0% / 100.0% / 100.0% | 100.0% / 100.0% / 100.0% | — / 0.0% / — | — / 0.0% / — |
| worker | 1 | 100.0% / 100.0% / 100.0% | 50.0% / 25.0% / 33.3% | 100.0% / 25.0% / 40.0% | 50.0% / 25.0% / 33.3% | 50.0% / 25.0% / 33.3% |

---

## 5. False positives (protected code reported as dead)

### shrike-cli — 0 symbol false positives

None.

### shrike-cli — 0 file false positives (strict)

None.

### fallow — 68 symbol false positives

| Case | Category | Symbol |
|---|---|---|
| const-namespace-dynamic-import | dynamic-import | `src/reassigned.ts#zeta` |
| test-entry-1 | test-entry | `src/helper.ts#helper1` |
| test-entry-2 | test-entry | `src/helper.ts#helper2` |
| test-entry-3 | test-entry | `src/helper.ts#helper3` |
| test-entry-4 | test-entry | `src/helper.ts#helper4` |
| test-entry-5 | test-entry | `src/helper.ts#helper5` |
| test-entry-6 | test-entry | `src/helper.ts#helper6` |
| test-entry-7 | test-entry | `src/helper.ts#helper7` |
| test-entry-8 | test-entry | `src/helper.ts#helper8` |
| test-entry-unused-export | test-entry | `__mocks__/client.ts#mockValue` |
| test-entry-unused-export | test-entry | `src/Button.stories.tsx#Primary` |
| test-entry-unused-export | test-entry | `src/Button.stories.tsx#default` |
| test-entry-unused-export | test-entry | `src/helper.ts#usedHelper` |
| mdx-two-sided | mdx | `components/widget.tsx#Widget` |
| next-image-two-sided | next-image-metadata | `app/icon.tsx#alt` |
| expo-two-sided | expo | `app/_layout.tsx#default` |
| expo-two-sided | expo | `app/index.tsx#default` |
| expo-two-sided | expo | `app/profile.tsx#default` |
| react-router-v7-routes | react-router | `app/routes/about.tsx#unstable_clientMiddleware` |
| react-router-v7-routes | react-router | `app/routes/home.tsx#unstable_middleware` |
| remix-routes-without-imports | remix | `app/root.tsx#CatchBoundary` |
| remix-routes-without-imports | remix | `app/root.tsx#shouldReload` |
| remix-routes-without-imports | remix | `app/root.tsx#unstable_shouldReload` |
| remix-custom-server | remix | `server-lib.ts#startServer` |
| next-route-segment-config | next-segment-config | `app/layout.tsx#ensureStatic` |
| next-route-segment-config | next-segment-config | `app/layout.tsx#unstable_instant` |
| next-route-segment-config | next-segment-config | `app/layout.tsx#unstable_prefetch` |
| next-config-references | next-config | `cache-handler.ts#default` |
| next-config-references | next-config | `image-loader.ts#default` |
| next-config-references | next-config | `loaders/svg-loader.ts#default` |
| next-config-references | next-config | `loaders/svg-loader.ts#raw` |
| expo-api-routes | expo | `app/+native-intent.tsx#legacy_subscribe` |
| expo-api-routes | expo | `app/+native-intent.tsx#redirectSystemPath` |
| expo-api-routes | expo | `app/hello+api.ts#GET` |
| expo-api-routes | expo | `app/hello+api.ts#POST` |
| require-context-1 | dynamic-import | `src/icons/nested/arrow.tsx#ArrowIcon` |
| require-context-1 | dynamic-import | `src/icons/star.tsx#StarIcon` |
| react17-legacy-root | react | `src/App.js#default` |
| react17-legacy-root | react | `src/client.js#__module__` |
| react17-legacy-root | react | `src/format.js#formatting` |
| … | | 28 more |

### fallow — 1 file false positives (strict)

| Case | Category | File |
|---|---|---|
| unused-files | unused-file | `src/transitively-dead.ts` |

### knip — 82 symbol false positives

| Case | Category | Symbol |
|---|---|---|
| const-namespace-dynamic-import | dynamic-import | `src/reassigned.ts#zeta` |
| test-entry-1 | test-entry | `src/helper.ts#helper1` |
| test-entry-2 | test-entry | `src/helper.ts#helper2` |
| test-entry-3 | test-entry | `src/helper.ts#helper3` |
| test-entry-4 | test-entry | `src/helper.ts#helper4` |
| test-entry-5 | test-entry | `src/helper.ts#helper5` |
| test-entry-6 | test-entry | `src/helper.ts#helper6` |
| test-entry-7 | test-entry | `src/helper.ts#helper7` |
| test-entry-8 | test-entry | `src/helper.ts#helper8` |
| test-entry-unused-export | test-entry | `__mocks__/client.ts#mockValue` |
| test-entry-unused-export | test-entry | `src/Button.stories.tsx#Primary` |
| test-entry-unused-export | test-entry | `src/Button.stories.tsx#default` |
| test-entry-unused-export | test-entry | `src/helper.ts#usedHelper` |
| config-entry-1 | config-entry | `next.config.ts#__module__` |
| config-entry-2 | config-entry | `next.config.ts#__module__` |
| config-entry-3 | config-entry | `next.config.ts#__module__` |
| config-entry-4 | config-entry | `next.config.ts#__module__` |
| config-entry-5 | config-entry | `next.config.ts#__module__` |
| config-entry-6 | config-entry | `next.config.ts#__module__` |
| config-entry-7 | config-entry | `next.config.ts#__module__` |
| config-entry-8 | config-entry | `next.config.ts#__module__` |
| mdx-two-sided | mdx | `components/widget.tsx#Widget` |
| type-only-transitive | internal-only | `src/host.ts#Image` |
| remix-two-sided | remix | `app/root.tsx#default` |
| remix-two-sided | remix | `app/root.tsx#loader` |
| remix-two-sided | remix | `app/routes/users.tsx#default` |
| remix-two-sided | remix | `app/routes/users.tsx#loader` |
| tanstack-two-sided | tanstack | `src/routes/posts.tsx#Route` |
| sveltekit-two-sided | sveltekit | `src/routes/+page.svelte#__module__` |
| sveltekit-two-sided | sveltekit | `src/routes/+page.ts#load` |
| react-router-v7-routes | react-router | `app/root.tsx#Layout` |
| react-router-v7-routes | react-router | `app/routes.ts#default` |
| react-router-v7-routes | react-router | `app/routes/about.tsx#action` |
| react-router-v7-routes | react-router | `app/routes/about.tsx#clientMiddleware` |
| react-router-v7-routes | react-router | `app/routes/about.tsx#default` |
| react-router-v7-routes | react-router | `app/routes/about.tsx#unstable_clientMiddleware` |
| react-router-v7-routes | react-router | `app/routes/home.tsx#default` |
| react-router-v7-routes | react-router | `app/routes/home.tsx#loader` |
| react-router-v7-routes | react-router | `app/routes/home.tsx#middleware` |
| react-router-v7-routes | react-router | `app/routes/home.tsx#unstable_middleware` |
| … | | 42 more |

### knip — 1 file false positives (strict)

| Case | Category | File |
|---|---|---|
| unused-files | unused-file | `src/transitively-dead.ts` |

### ts-prune — 183 symbol false positives

| Case | Category | Symbol |
|---|---|---|
| page-entry-1 | page-entry | `app/page.tsx#default` |
| page-entry-2 | page-entry | `app/page.tsx#default` |
| page-entry-3 | page-entry | `app/page.tsx#default` |
| page-entry-4 | page-entry | `app/page.tsx#default` |
| page-entry-5 | page-entry | `app/page.tsx#default` |
| page-entry-6 | page-entry | `app/page.tsx#default` |
| page-entry-7 | page-entry | `app/page.tsx#default` |
| page-entry-8 | page-entry | `app/page.tsx#default` |
| use-client-1 | use-client | `components/Widget.tsx#Widget1` |
| use-client-2 | use-client | `components/Widget.tsx#Widget2` |
| use-client-3 | use-client | `components/Widget.tsx#Widget3` |
| use-client-4 | use-client | `components/Widget.tsx#Widget4` |
| use-client-5 | use-client | `components/Widget.tsx#Widget5` |
| use-client-6 | use-client | `components/Widget.tsx#Widget6` |
| use-client-7 | use-client | `components/Widget.tsx#Widget7` |
| use-client-8 | use-client | `components/Widget.tsx#Widget8` |
| test-entry-unused-export | test-entry | `__mocks__/client.ts#mockValue` |
| test-entry-unused-export | test-entry | `src/Button.stories.tsx#Primary` |
| test-entry-unused-export | test-entry | `src/Button.stories.tsx#default` |
| package-entry-1 | package-entry | `src/index.ts#api1` |
| package-entry-2 | package-entry | `src/index.ts#api2` |
| package-entry-3 | package-entry | `src/index.ts#api3` |
| package-entry-4 | package-entry | `src/index.ts#api4` |
| package-entry-5 | package-entry | `src/index.ts#api5` |
| package-entry-6 | package-entry | `src/index.ts#api6` |
| package-entry-7 | package-entry | `src/index.ts#api7` |
| package-entry-8 | package-entry | `src/index.ts#api8` |
| jsx-1 | jsx | `components/Button.tsx#Button1` |
| jsx-2 | jsx | `components/Button.tsx#Button2` |
| jsx-3 | jsx | `components/Button.tsx#Button3` |
| jsx-4 | jsx | `components/Button.tsx#Button4` |
| jsx-5 | jsx | `components/Button.tsx#Button5` |
| jsx-6 | jsx | `components/Button.tsx#Button6` |
| jsx-7 | jsx | `components/Button.tsx#Button7` |
| jsx-8 | jsx | `components/Button.tsx#Button8` |
| default-reexport-1 | default-reexport | `src/impl.tsx#default` |
| default-reexport-2 | default-reexport | `src/impl.tsx#default` |
| default-reexport-3 | default-reexport | `src/impl.tsx#default` |
| default-reexport-4 | default-reexport | `src/impl.tsx#default` |
| default-reexport-5 | default-reexport | `src/impl.tsx#default` |
| … | | 143 more |

### ts-prune — 0 file false positives (strict)

None.

### ts-unused-exports — 160 symbol false positives

| Case | Category | Symbol |
|---|---|---|
| page-entry-1 | page-entry | `app/page.tsx#default` |
| page-entry-2 | page-entry | `app/page.tsx#default` |
| page-entry-3 | page-entry | `app/page.tsx#default` |
| page-entry-4 | page-entry | `app/page.tsx#default` |
| page-entry-5 | page-entry | `app/page.tsx#default` |
| page-entry-6 | page-entry | `app/page.tsx#default` |
| page-entry-7 | page-entry | `app/page.tsx#default` |
| page-entry-8 | page-entry | `app/page.tsx#default` |
| dynamic-import-1 | dynamic-import | `src/feature.ts#feature1` |
| dynamic-import-2 | dynamic-import | `src/feature.ts#feature2` |
| dynamic-import-3 | dynamic-import | `src/feature.ts#feature3` |
| dynamic-import-4 | dynamic-import | `src/feature.ts#feature4` |
| dynamic-import-5 | dynamic-import | `src/feature.ts#feature5` |
| dynamic-import-6 | dynamic-import | `src/feature.ts#feature6` |
| dynamic-import-7 | dynamic-import | `src/feature.ts#feature7` |
| dynamic-import-8 | dynamic-import | `src/feature.ts#feature8` |
| promise-all-dynamic | dynamic-import | `src/mod.ts#used` |
| then-dynamic-import | dynamic-import | `src/escaped.ts#alsoAny` |
| then-dynamic-import | dynamic-import | `src/escaped.ts#any` |
| const-namespace-dynamic-import | dynamic-import | `src/escaped.ts#alsoAny` |
| const-namespace-dynamic-import | dynamic-import | `src/escaped.ts#any` |
| const-namespace-dynamic-import | dynamic-import | `src/mod.ts#used` |
| const-namespace-dynamic-import | dynamic-import | `src/reassigned.ts#epsilon` |
| const-namespace-dynamic-import | dynamic-import | `src/reassigned.ts#zeta` |
| test-entry-unused-export | test-entry | `__mocks__/client.ts#mockValue` |
| test-entry-unused-export | test-entry | `src/Button.stories.tsx#Primary` |
| test-entry-unused-export | test-entry | `src/Button.stories.tsx#default` |
| package-entry-1 | package-entry | `src/index.ts#api1` |
| package-entry-2 | package-entry | `src/index.ts#api2` |
| package-entry-3 | package-entry | `src/index.ts#api3` |
| package-entry-4 | package-entry | `src/index.ts#api4` |
| package-entry-5 | package-entry | `src/index.ts#api5` |
| package-entry-6 | package-entry | `src/index.ts#api6` |
| package-entry-7 | package-entry | `src/index.ts#api7` |
| package-entry-8 | package-entry | `src/index.ts#api8` |
| parallel-routes-exp-1 | parallel-routes | `app/@modal/(.)photo/[id]/page.tsx#default` |
| parallel-routes-exp-2 | parallel-routes | `app/@modal/(.)photo/[id]/page.tsx#default` |
| parallel-routes-exp-3 | parallel-routes | `app/@modal/(.)photo/[id]/page.tsx#default` |
| parallel-routes-exp-4 | parallel-routes | `app/@modal/(.)photo/[id]/page.tsx#default` |
| parallel-routes-exp-5 | parallel-routes | `app/@modal/(.)photo/[id]/page.tsx#default` |
| … | | 120 more |

### ts-unused-exports — 0 file false positives (strict)

None.

---

## 6. Missed must-report items

### shrike-cli — 0 missed symbols

None.

### shrike-cli — 0 missed unused files

None.

### fallow — 16 missed symbols

| Case | Category | Symbol |
|---|---|---|
| mdx-components-gate | mdx | `mdx-components.tsx#useMDXComponents` |
| worker-entry | worker | `app/components/heavy-worker.ts#unusedWorkerHelper` |
| worker-entry | worker | `app/components/processor.ts#unusedWorkletHelper` |
| worker-entry | worker | `app/components/service-worker.ts#unusedServiceWorkerHelper` |
| next-dotted-page-extensions | next-pages | `app/layout.page.tsx#unusedLayoutHelper` |
| next-dotted-page-extensions | next-pages | `app/page.page.tsx#unusedPageHelper` |
| next-dotted-page-extensions | next-pages | `app/route.page.tsx#unusedRouteHelper` |
| local-export-alias | barrel | `src/impl.ts#dead` |
| gatsby-templates | gatsby | `src/templates/dead-template.tsx#default` |
| gatsby-templates | gatsby | `src/templates/dead-template.tsx#query` |
| ladle-stories | ladle | `.ladle/shell.tsx#unusedShellHelper` |
| unreferenced-scripts | script-entry | `scripts/used-by-package.ts#unusedPackageScriptExport` |
| unused-component-prop-1 | react-component-prop | `components/Internal.tsx#Dead` |
| unused-page-data-key-1 | next-page-data | `pages/about.tsx#unusedAboutHelper` |
| unused-page-data-key-1 | next-page-data | `pages/index.tsx#unusedPageHelper` |
| unused-namespace-member-1 | unused-namespace-member | `src/legacy.ts#LocalOnly` |

### fallow — 0 missed unused files

None.

### knip — 47 missed symbols

| Case | Category | Symbol |
|---|---|---|
| mdx-two-sided | mdx | `mdx-components.tsx#unusedMdxHelper` |
| next-image-two-sided | next-image-metadata | `app/icon.tsx#unusedIconHelper` |
| next-image-two-sided | next-image-metadata | `app/page.tsx#size` |
| next-interrupt-two-sided | next-interrupt | `app/forbidden.tsx#unusedForbiddenHelper` |
| next-interrupt-two-sided | next-interrupt | `app/unauthorized.tsx#unusedUnauthorizedHelper` |
| expo-two-sided | expo | `app/profile.tsx#unusedProfileHelper` |
| astro-two-sided | astro | `src/pages/health.ts#unusedEndpointHelper` |
| next-pages-report-web-vitals | next-pages | `pages/_app.tsx#unusedAppExport` |
| next-route-segment-config | next-segment-config | `app/layout.tsx#unusedLayoutHelper` |
| next-route-segment-config | next-segment-config | `app/page.tsx#unusedPageHelper` |
| expo-api-routes | expo | `app/+native-intent.tsx#unusedIntentHelper` |
| expo-api-routes | expo | `app/hello+api.ts#unusedApiHelper` |
| expo-custom-root | expo | `src/routes/_layout.tsx#unusedLayoutHelper` |
| expo-custom-root | expo | `src/routes/index.tsx#unusedHomeHelper` |
| expo-custom-root | expo | `src/routes/user/[id].tsx#unusedUserHelper` |
| worker-entry | worker | `app/components/heavy-worker.ts#unusedWorkerHelper` |
| worker-entry | worker | `app/components/processor.ts#unusedWorkletHelper` |
| worker-entry | worker | `app/components/service-worker.ts#unusedServiceWorkerHelper` |
| next-dotted-page-extensions | next-pages | `app/layout.page.tsx#unusedLayoutHelper` |
| next-dotted-page-extensions | next-pages | `app/page.page.tsx#unusedPageHelper` |
| next-dotted-page-extensions | next-pages | `app/route.page.tsx#unusedRouteHelper` |
| next-custom-server | next-config | `server.js#start` |
| next-custom-server | next-config | `server.js#unusedServerHelper` |
| next-param-matching | next-segment-config | `app/layout.tsx#unusedLayoutHelper` |
| next-param-matching | next-segment-config | `app/loading.tsx#unstable_generateParamMatching` |
| next-param-matching | next-segment-config | `app/loading.tsx#unstable_paramMatching` |
| next-param-matching | next-segment-config | `app/page.tsx#unusedPageHelper` |
| next-param-matching | next-segment-config | `app/route.ts#unstable_generateParamMatching` |
| next-param-matching | next-segment-config | `app/route.ts#unstable_paramMatching` |
| react-cosmos-fixtures | react-cosmos | `decorators/cosmos.decorator.tsx#unusedDecoratorHelper` |
| react-cosmos-custom-config | react-cosmos | `src/IgnoredThing.tsx#IgnoredThing` |
| react-cosmos-custom-config | react-cosmos | `src/examples/Thing.skip.tsx#Skipped` |
| gatsby-templates | gatsby | `src/templates/blog-post.tsx#unusedTemplateHelper` |
| gatsby-templates | gatsby | `src/templates/dead-template.tsx#default` |
| gatsby-templates | gatsby | `src/templates/dead-template.tsx#query` |
| gatsby-templates | gatsby | `src/templates/dead-template.tsx#unusedDeadHelper` |
| next-instrumentation | next-instrumentation | `instrumentation-client.ts#unusedClientHelper` |
| next-instrumentation | next-instrumentation | `instrumentation.ts#unusedInstrumentationHelper` |
| monorepo-next-paths | monorepo-next-paths | `apps/web/app/page.tsx#unusedWebHelper` |
| unreferenced-scripts | script-entry | `scripts/used-by-package.ts#unusedPackageScriptExport` |
| … | | 7 more |

### knip — 0 missed unused files

None.

### ts-prune — 96 missed symbols

| Case | Category | Symbol |
|---|---|---|
| promise-all-dynamic | dynamic-import | `src/mod.ts#dead` |
| then-dynamic-import | dynamic-import | `src/mod.ts#dead` |
| const-namespace-dynamic-import | dynamic-import | `src/mod.ts#dead` |
| next-image-two-sided | next-image-metadata | `app/icon.tsx#unusedIconHelper` |
| next-image-two-sided | next-image-metadata | `app/page.tsx#size` |
| next-interrupt-two-sided | next-interrupt | `app/forbidden.tsx#unusedForbiddenHelper` |
| next-interrupt-two-sided | next-interrupt | `app/unauthorized.tsx#unusedUnauthorizedHelper` |
| internal-only-two-sided | internal-only | `src/registry.ts#unusedExport` |
| internal-only-types | internal-only | `src/card.ts#unusedCardHelper` |
| type-only-transitive | internal-only | `src/host.ts#unusedHostHelper` |
| remix-two-sided | remix | `app/routes/users.tsx#unusedRouteHelper` |
| tanstack-two-sided | tanstack | `src/routes/posts.tsx#unusedRouteHelper` |
| expo-two-sided | expo | `app/profile.tsx#unusedProfileHelper` |
| sveltekit-two-sided | sveltekit | `src/lib/unused.ts#unusedLib` |
| sveltekit-two-sided | sveltekit | `src/routes/+page.ts#unusedRouteHelper` |
| astro-two-sided | astro | `src/pages/health.ts#unusedEndpointHelper` |
| nuxt-two-sided | nuxt | `server/models/unused.ts#unusedModel` |
| remix-routes-without-imports | remix | `app/components/dead.tsx#deadThing` |
| tanstack-custom-routes-dir | tanstack | `src/components/dead.tsx#deadThing` |
| next-route-segment-config | next-segment-config | `app/layout.tsx#unusedLayoutHelper` |
| next-route-segment-config | next-segment-config | `app/page.tsx#unusedPageHelper` |
| next-config-references | next-config | `lib/commented.ts#commentedReference` |
| next-config-references | next-config | `lib/dead.ts#deadThing` |
| expo-api-routes | expo | `app/+native-intent.tsx#unusedIntentHelper` |
| expo-api-routes | expo | `app/hello+api.ts#unusedApiHelper` |
| expo-custom-root | expo | `app/dead.tsx#deadOutsideRoot` |
| expo-custom-root | expo | `src/components/dead.tsx#deadThing` |
| expo-custom-root | expo | `src/routes/_layout.tsx#unusedLayoutHelper` |
| expo-custom-root | expo | `src/routes/index.tsx#unusedHomeHelper` |
| expo-custom-root | expo | `src/routes/user/[id].tsx#unusedUserHelper` |
| expo-config-plugins | expo | `plugins/unused.ts#default` |
| expo-config-plugins | expo | `src/dead.ts#deadThing` |
| react-router-custom-app-dir | react-router | `src/components/dead.tsx#deadThing` |
| react-router-custom-app-dir | react-router | `src/frontend/root.tsx#unusedRootHelper` |
| react-router-custom-app-dir | react-router | `src/frontend/route-modules/about.tsx#unusedAboutHelper` |
| react-router-custom-app-dir | react-router | `src/frontend/route-modules/unreferenced.tsx#default` |
| react-router-custom-app-dir | react-router | `src/frontend/route-modules/unreferenced.tsx#unusedUnreferencedHelper` |
| react-router-custom-app-dir | react-router | `src/frontend/routes/home.tsx#unusedRouteHelper` |
| import-meta-glob-1 | dynamic-import | `src/components/dead.tsx#DeadComponent` |
| import-meta-glob-1 | dynamic-import | `src/pages/skip.tsx#Skipped` |
| … | | 56 more |

### ts-prune — 0 missed unused files

None.

### ts-unused-exports — 18 missed symbols

| Case | Category | Symbol |
|---|---|---|
| worker-entry | worker | `app/components/heavy-worker.ts#unusedWorkerHelper` |
| worker-entry | worker | `app/components/processor.ts#unusedWorkletHelper` |
| worker-entry | worker | `app/components/service-worker.ts#unusedServiceWorkerHelper` |
| next-custom-server | next-config | `server.js#start` |
| next-custom-server | next-config | `server.js#unusedServerHelper` |
| barrel-alias-chain | barrel | `src/barrel.ts#dead` |
| barrel-alias-chain | barrel | `src/barrel.ts#used` |
| barrel-alias-chain | barrel | `src/impl.ts#dead` |
| local-export-alias | barrel | `src/impl.ts#dead` |
| react-cosmos-custom-config | react-cosmos | `src/IgnoredThing.tsx#IgnoredThing` |
| react-cosmos-custom-config | react-cosmos | `src/Legacy.tsx#Legacy` |
| react-cosmos-custom-config | react-cosmos | `src/cosmos.decorator.js#unusedDecoratorHelper` |
| ladle-stories | ladle | `.ladle/shell.tsx#unusedShellHelper` |
| unused-files | unused-file | `src/transitively-dead.ts#transitivelyDead` |
| unused-component-prop-1 | react-component-prop | `components/Internal.tsx#Dead` |
| unused-page-data-key-1 | next-page-data | `pages/about.tsx#unusedAboutHelper` |
| unused-page-data-key-1 | next-page-data | `pages/index.tsx#unusedPageHelper` |
| unused-namespace-member-1 | unused-namespace-member | `src/legacy.ts#LocalOnly` |

### ts-unused-exports — 0 missed unused files

None.

---

## 7. Invalid configurations

| Case | Category | Tool | Error (tail) |
|---|---|---|---|
| react-router-custom-app-dir | react-router | knip | ERROR: Error loading react-router.config.ts (Cannot find module '@react-router/dev/routes') ERROR: Please fix or visit https://knip.dev/reference/known-issues |
| next-image-two-sided | next-image-metadata | ts-unused-exports | onfig file. For example, given: / |-- config | -- tsconfig.json -- src -- file.ts Then the usage would be: ts-unused-exports config/tsconfig.json ../src/file.ts |
| next-interrupt-two-sided | next-interrupt | ts-unused-exports | onfig file. For example, given: / |-- config | -- tsconfig.json -- src -- file.ts Then the usage would be: ts-unused-exports config/tsconfig.json ../src/file.ts |
| internal-only-two-sided | internal-only | ts-unused-exports | onfig file. For example, given: / |-- config | -- tsconfig.json -- src -- file.ts Then the usage would be: ts-unused-exports config/tsconfig.json ../src/file.ts |
| internal-only-types | internal-only | ts-unused-exports | onfig file. For example, given: / |-- config | -- tsconfig.json -- src -- file.ts Then the usage would be: ts-unused-exports config/tsconfig.json ../src/file.ts |
| type-only-transitive | internal-only | ts-unused-exports | onfig file. For example, given: / |-- config | -- tsconfig.json -- src -- file.ts Then the usage would be: ts-unused-exports config/tsconfig.json ../src/file.ts |
| remix-two-sided | remix | ts-unused-exports | onfig file. For example, given: / |-- config | -- tsconfig.json -- src -- file.ts Then the usage would be: ts-unused-exports config/tsconfig.json ../src/file.ts |
| tanstack-two-sided | tanstack | ts-unused-exports | onfig file. For example, given: / |-- config | -- tsconfig.json -- src -- file.ts Then the usage would be: ts-unused-exports config/tsconfig.json ../src/file.ts |
| expo-two-sided | expo | ts-unused-exports | onfig file. For example, given: / |-- config | -- tsconfig.json -- src -- file.ts Then the usage would be: ts-unused-exports config/tsconfig.json ../src/file.ts |
| sveltekit-two-sided | sveltekit | ts-unused-exports | onfig file. For example, given: / |-- config | -- tsconfig.json -- src -- file.ts Then the usage would be: ts-unused-exports config/tsconfig.json ../src/file.ts |
| astro-two-sided | astro | ts-unused-exports | onfig file. For example, given: / |-- config | -- tsconfig.json -- src -- file.ts Then the usage would be: ts-unused-exports config/tsconfig.json ../src/file.ts |
| nuxt-two-sided | nuxt | ts-unused-exports | onfig file. For example, given: / |-- config | -- tsconfig.json -- src -- file.ts Then the usage would be: ts-unused-exports config/tsconfig.json ../src/file.ts |
| remix-routes-without-imports | remix | ts-unused-exports | onfig file. For example, given: / |-- config | -- tsconfig.json -- src -- file.ts Then the usage would be: ts-unused-exports config/tsconfig.json ../src/file.ts |
| tanstack-custom-routes-dir | tanstack | ts-unused-exports | onfig file. For example, given: / |-- config | -- tsconfig.json -- src -- file.ts Then the usage would be: ts-unused-exports config/tsconfig.json ../src/file.ts |
| next-route-segment-config | next-segment-config | ts-unused-exports | onfig file. For example, given: / |-- config | -- tsconfig.json -- src -- file.ts Then the usage would be: ts-unused-exports config/tsconfig.json ../src/file.ts |
| next-config-references | next-config | ts-unused-exports | onfig file. For example, given: / |-- config | -- tsconfig.json -- src -- file.ts Then the usage would be: ts-unused-exports config/tsconfig.json ../src/file.ts |
| expo-api-routes | expo | ts-unused-exports | onfig file. For example, given: / |-- config | -- tsconfig.json -- src -- file.ts Then the usage would be: ts-unused-exports config/tsconfig.json ../src/file.ts |
| expo-custom-root | expo | ts-unused-exports | onfig file. For example, given: / |-- config | -- tsconfig.json -- src -- file.ts Then the usage would be: ts-unused-exports config/tsconfig.json ../src/file.ts |
| expo-config-plugins | expo | ts-unused-exports | onfig file. For example, given: / |-- config | -- tsconfig.json -- src -- file.ts Then the usage would be: ts-unused-exports config/tsconfig.json ../src/file.ts |
| react-router-custom-app-dir | react-router | ts-unused-exports | onfig file. For example, given: / |-- config | -- tsconfig.json -- src -- file.ts Then the usage would be: ts-unused-exports config/tsconfig.json ../src/file.ts |
| import-meta-glob-1 | dynamic-import | ts-unused-exports | onfig file. For example, given: / |-- config | -- tsconfig.json -- src -- file.ts Then the usage would be: ts-unused-exports config/tsconfig.json ../src/file.ts |
| require-context-1 | dynamic-import | ts-unused-exports | onfig file. For example, given: / |-- config | -- tsconfig.json -- src -- file.ts Then the usage would be: ts-unused-exports config/tsconfig.json ../src/file.ts |
| next-intl-conventions | next-config | ts-unused-exports | onfig file. For example, given: / |-- config | -- tsconfig.json -- src -- file.ts Then the usage would be: ts-unused-exports config/tsconfig.json ../src/file.ts |
| vite-html-entry-1 | vite | ts-unused-exports | onfig file. For example, given: / |-- config | -- tsconfig.json -- src -- file.ts Then the usage would be: ts-unused-exports config/tsconfig.json ../src/file.ts |
| vite-root-entry-1 | vite | ts-unused-exports | onfig file. For example, given: / |-- config | -- tsconfig.json -- src -- file.ts Then the usage would be: ts-unused-exports config/tsconfig.json ../src/file.ts |
| react17-legacy-root | react | ts-unused-exports | onfig file. For example, given: / |-- config | -- tsconfig.json -- src -- file.ts Then the usage would be: ts-unused-exports config/tsconfig.json ../src/file.ts |
| storybook-mdx-docs | storybook | ts-unused-exports | onfig file. For example, given: / |-- config | -- tsconfig.json -- src -- file.ts Then the usage would be: ts-unused-exports config/tsconfig.json ../src/file.ts |
| next-page-extensions-md | next-pages | ts-unused-exports | onfig file. For example, given: / |-- config | -- tsconfig.json -- src -- file.ts Then the usage would be: ts-unused-exports config/tsconfig.json ../src/file.ts |
| webpack-entries | webpack | ts-unused-exports | onfig file. For example, given: / |-- config | -- tsconfig.json -- src -- file.ts Then the usage would be: ts-unused-exports config/tsconfig.json ../src/file.ts |
| storybook-custom-stories | storybook | ts-unused-exports | onfig file. For example, given: / |-- config | -- tsconfig.json -- src -- file.ts Then the usage would be: ts-unused-exports config/tsconfig.json ../src/file.ts |
| next-param-matching | next-segment-config | ts-unused-exports | onfig file. For example, given: / |-- config | -- tsconfig.json -- src -- file.ts Then the usage would be: ts-unused-exports config/tsconfig.json ../src/file.ts |
| cypress-tests | test-entry | ts-unused-exports | onfig file. For example, given: / |-- config | -- tsconfig.json -- src -- file.ts Then the usage would be: ts-unused-exports config/tsconfig.json ../src/file.ts |
| unreferenced-scripts | script-entry | ts-unused-exports | onfig file. For example, given: / |-- config | -- tsconfig.json -- src -- file.ts Then the usage would be: ts-unused-exports config/tsconfig.json ../src/file.ts |

---

## 8. Caveats

1. **The corpus is Shrike's own CI gate, not a random sample.** It over-represents framework conventions (Next.js routes, server actions, barrels, dynamic imports) because those are where false positives are costly. Shrike's perfect score is therefore expected by construction and is not independent evidence; the competitor scores on the same labels are the informative comparison. The per-category table is the right way to read it; the headline is not a claim about an arbitrary repository.
2. **Different categories.** Knip and fallow's unused exports include de-export candidates; Shrike's `internalOnly` is advisory and is not scored against them. Member-level warnings are counted separately.
3. **Framework support differs.** A tool that does not implement a convention will report the convention's entry as dead. That is a real false positive for that tool, but it is also a scope difference. The category table makes it visible.
4. **No dependency labels.** The corpus labels symbols and files, not package dependencies. Dependency accuracy remains the agreement study in the performance report.
5. **Minimal projects.** Each case is small and has no installed dependencies. The corpus tests conventions and reachability, not resolver performance or monorepo scale. Performance is measured separately on 12 real and synthetic fixtures.
6. **Closure allowance.** A used file whose every importer is reported unused is transitively dead. The strict column counts it as a false positive; the primary score does not. Both are shown.

---

## 9. Reproduction

```bash
# Build Shrike
cargo build --release -p shrike-engine
pnpm --filter shrike build

# Install competitors (see benchmarks/competitors/setup.sh)
benchmarks/competitors/setup.sh

# Run the labeled accuracy suite
python3 benchmarks/competitors/accuracy.py --work /tmp/opencode/bench-v2

# Quick smoke run
python3 benchmarks/competitors/accuracy.py --limit 20 --tools shrike-cli,fallow,knip
```

Raw per-case results are in the JSON next to this report. Raw tool output is deleted unless `--keep-raw` is passed.
