# PLAN-4 — Build Graph and Next.js

> **Parent:** [MASTER_PROMPT.md](../MASTER_PROMPT.md) · **Tracker:** [TRACKER.md](../TRACKER.md)
> **Status:** `planned` · **Depends on:** PLAN-1, PLAN-3 · **Unblocks:** PLAN-5
> **Owner prompt:** [`../prompts/01-rust-engine-agent.md`](../prompts/01-rust-engine-agent.md)

---

## 1. Goal

Add build-graph evidence and deepen the Next.js adapter so that Shrike can say:

> This dead subtree is in no client bundle, it costs 41 KB across 6 routes, and
> the build graph confirms the static claim.

The build graph is optional evidence. The Next.js semantics are mandatory
correctness.

---

## 2. Scope

### In scope

- `next analyze export` JSONL ingestion.
- Version guard and graceful absence.
- Module-to-file join.
- Presence/absence, route attribution, byte impact.
- Dynamic-edge feedback from the build graph.
- Server-action reachability via the client-reference manifest.
- Client-boundary diagnostics.
- Fallback behavior for Next.js 15 and unsupported versions.
- Receipt integration and corpus runs with and without evidence.

### Out of scope

- Bundle optimization suggestions.
- Lazy-loading or code-splitting advice.
- RSC migration guidance.
- Running `next build` on every scan (only with `--build`).

---

## 3. Work breakdown

### 4.1 — Snapshot discovery

**Task:** Find an existing build snapshot or produce one with `--build`.

**Acceptance:**

- Default locations are searched (`.next/analyze/`, `.shrike/build/`, a
  configurable path).
- `--build` runs the project's build command with a timeout and then locates
  the export.
- Without `--build`, a missing snapshot is a status, not a crash.
- The snapshot path, mtime, and source hash are recorded.
- The command never modifies `next.config`.

**Read:** [`../docs/06-build-graph-and-nextjs.md`](../docs/06-build-graph-and-nextjs.md) §2–§3.

**Evidence:** integration test with and without `--build`.

### 4.2 — Version guard

**Task:** Detect the Next.js version and choose a capability level.

**Acceptance:**

- 16.4+ → full.
- 16.1–16.3 → partial.
- 15.x → static-only with a warning.
- < 15 → static-only, unsupported.
- If `package.json` and the snapshot disagree, the lower capability wins.
- An unparseable version is unsupported, not assumed current.

**Read:** [`../docs/06-build-graph-and-nextjs.md`](../docs/06-build-graph-and-nextjs.md) §2.2.

**Evidence:** unit tests for each version.

### 4.3 — JSONL parser

**Task:** Stream and parse the module/chunk/entry/asset/client-reference records.

**Acceptance:**

- One JSON object per line; no full-file load.
- Additive fields are ignored.
- A malformed line is recorded and the parser continues or fails to
  `malformed`, per policy.
- Unknown record types are skipped and counted.
- The parser is bounded in memory and handles a 200 MB snapshot.

**Read:** [`../docs/06-build-graph-and-nextjs.md`](../docs/06-build-graph-and-nextjs.md) §2.1.

**Evidence:** parser fixtures, including malformed and unknown records.

### 4.4 — Module index

**Task:** Build path and ID indexes with normalization.

**Acceptance:**

- Strips loader prefixes, query strings, and `turbopack://` prefixes.
- Normalizes to repo-relative `/` paths.
- Matches exact path first, then longest suffix.
- Unmatched modules are recorded, never guessed.
- Duplicate paths are resolved deterministically (first by module ID).

**Read:** [`../docs/06-build-graph-and-nextjs.md`](../docs/06-build-graph-and-nextjs.md) §3.1–§3.2.

**Evidence:** unit tests with virtual and loader-prefixed paths.

### 4.5 — Module-to-file join

**Task:** Join build modules to static files and report coverage.

**Acceptance:**

- Join coverage is computed and reported.
- ≥ 0.95 → full; 0.80–0.95 → partial; < 0.80 → low-coverage.
- Unjoined modules are listed with a reason.
- The join does not invent a path.
- On the reference app, coverage is ≥ 0.90.

**Read:** [`../docs/06-build-graph-and-nextjs.md`](../docs/06-build-graph-and-nextjs.md) §3.3.

**Evidence:** integration test on a real app.

### 4.6 — Presence and absence

**Task:** Determine whether a candidate module is present in a chunk.

**Acceptance:**

- Client presence → `present`; deletion suppressed.
- Server-only presence → `server-only`; deletion needs source proof.
- Absent from every chunk and joined → `absent`.
- Unjoined → `unknown`; no confirmation.
- Presence beats the static graph.
- The verdict is recorded per candidate.

**Read:** [`../docs/06-build-graph-and-nextjs.md`](../docs/06-build-graph-and-nextjs.md) §4.

**Evidence:** fixture tests for each verdict.

### 4.7 — Route attribution

**Task:** Attribute modules and chunks to routes.

**Acceptance:**

- Route chunks map to their route.
- Shared chunks map to every route that loads them.
- Framework chunks are not attributed to user routes.
- Client and server chunks are recorded separately.
- Route groups and dynamic segments are preserved.
- A module in no route is reported as `unrouted`.

**Read:** [`../docs/06-build-graph-and-nextjs.md`](../docs/06-build-graph-and-nextjs.md) §6.

**Evidence:** route fixture with shared and route-specific chunks.

### 4.8 — Byte impact

**Task:** Report per-route and shared bytes without double counting.

**Acceptance:**

- Per-route bytes and shared bytes are separate fields.
- `totalRouteBytes` does not double count shared chunks.
- A partial attribution is labeled `partial` with a reason.
- A missing size is `unknown`, not zero.
- The receipt records the snapshot hash and method.

**Read:** [`../docs/06-build-graph-and-nextjs.md`](../docs/06-build-graph-and-nextjs.md) §6.1.

**Evidence:** arithmetic test on a shared-chunk fixture.

### 4.9 — Build-edge feedback

**Task:** Add certain edges from build evidence and re-run reachability.

**Acceptance:**

- A build edge can only add liveness.
- The edge is recorded with its chunk/route evidence.
- The candidate is suppressed.
- A test proves a false positive is caught by a build edge.
- Build edges never cause a deletion.

**Read:** [`../docs/06-build-graph-and-nextjs.md`](../docs/06-build-graph-and-nextjs.md) §5.

**Evidence:** `build-graph/dynamic-edge/` fixture.

### 4.10 — Server actions

**Task:** Use the client-reference manifest conservatively.

**Acceptance:**

- A `"use server"` export is an entry.
- A client-reference match creates a certain edge.
- Without a manifest, the action stays live.
- Absence from the manifest is not proof of death.
- A form action or fetch call in another package keeps the action live.

**Read:** [`../docs/06-build-graph-and-nextjs.md`](../docs/06-build-graph-and-nextjs.md) §7.1.

**Evidence:** `build-graph/server-action/` fixture.

### 4.11 — Client boundaries

**Task:** Model `"use client"` and `"use server"` boundaries with build layers.

**Acceptance:**

- A `"use client"` module is an entry only when imported or a route file.
- A boundary mismatch produces a diagnostic, not a finding.
- A server-only module in a client chunk produces `client-boundary-leak`.
- The diagnostic is opt-outable and does not block the run.

**Read:** [`../docs/06-build-graph-and-nextjs.md`](../docs/06-build-graph-and-nextjs.md) §7.2–§7.3.

**Evidence:** `build-graph/client-boundary/` fixture.

### 4.12 — Proxy, middleware, metadata

**Task:** Ensure these are entries and never candidates.

**Acceptance:**

- `proxy.ts`/`proxy.js` and `middleware.ts`/`middleware.js` exports are entries.
- Matchers are parsed but not evaluated.
- Instrumentation files are entries.
- Metadata route files and metadata exports are entries.
- A test proves none can be deleted.

**Read:** [`../docs/06-build-graph-and-nextjs.md`](../docs/06-build-graph-and-nextjs.md) §8.

**Evidence:** corpus entries; fixture test.

### 4.13 — Fallback matrix

**Task:** Implement every status in the fallback matrix.

**Acceptance:**

- `full`, `partial`, `low-coverage`, `absent`, `stale`, `malformed`,
  `unsupported`, `build-failed`.
- Each status has a test.
- `--require-build-evidence` turns unavailable evidence into exit 4.
- Static-only runs still produce a valid delta and plan.

**Read:** [`../docs/06-build-graph-and-nextjs.md`](../docs/06-build-graph-and-nextjs.md) §9.

**Evidence:** fallback test matrix.

### 4.14 — Receipt integration

**Task:** Record build evidence in the receipt.

**Acceptance:**

- Records source, snapshot hash, Next.js version, status, and join coverage.
- Absence is explicit (`status: absent`), not a missing field.
- A receipt with `--require-build-evidence` and no evidence is not `verified`.
- The schema validates both present and absent cases.

**Read:** [`../docs/06-build-graph-and-nextjs.md`](../docs/06-build-graph-and-nextjs.md) §10.

**Evidence:** schema tests.

### 4.15 — Corpus with and without evidence

**Task:** Run the must-not-report corpus in both modes.

**Acceptance:**

- Zero findings with build evidence.
- Zero findings without build evidence.
- The number of suppressed candidates is reported.
- A build-evidence-only false positive becomes a corpus entry.

**Read:** [`../docs/08-benchmarks-and-confidence.md`](../docs/08-benchmarks-and-confidence.md) §2.

**Evidence:** corpus report.

### 4.16 — Docs update

**Task:** Update `docs/06` to match the implemented behavior.

**Acceptance:**

- The version matrix, record shapes, and fallback statuses are accurate.
- Open questions for PLAN-4 are resolved or reassigned.
- The tracker links the updated doc.

**Evidence:** review.

---

## 4. Exit gate

- [ ] A real Next.js 16.4 app produces a full snapshot and join coverage ≥ 0.90.
- [ ] A module present in a client chunk suppresses deletion.
- [ ] An absent module confirms, but never authorizes, deletion.
- [ ] Bytes are per-route and shared, with no double counting.
- [ ] A build edge suppresses a false positive and is recorded.
- [ ] Server actions are never confirmed dead from absence.
- [ ] 16.1–16.3 and 15.x degrade cleanly.
- [ ] Missing, stale, malformed, and low-coverage snapshots are statuses.
- [ ] `--require-build-evidence` exits 4 when evidence is unavailable.
- [ ] The corpus is green with and without evidence.
- [ ] The receipt records build status, version, hash, and coverage.

---

## 5. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Export schema changes | Streaming parser with additive tolerance; version guard |
| Low join coverage | Suffix matching; unmatched report; no confirmation below threshold |
| Misleading byte claims | Per-route/shared separation; `partial` label |
| Build is slow | `--build` only; use an existing snapshot by default |
| Server action false negative | Conservative: absence is not death |
| Virtual modules | Record as unmatched; never guess a source path |
| Monorepo multiple builds | One snapshot per app; merge by app root (PLAN-6) |

---

## 6. Definition of done

On a real Next.js app, Shrike reports that a dead subtree is absent from every
client bundle, names the routes it used to affect, reports per-route and shared
bytes without double counting, and records the build snapshot hash and coverage
in the receipt — while still working, with a clear status, when no build evidence
exists.
