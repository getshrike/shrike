# 03 — Next.js Build Graph

> **Parent:** [README.md](README.md) · **Status:** research note · **Date:** 2026-11-20 · **Confidence:** medium
> **Method:** Next.js docs, release notes, and observed output shapes.

---

## 1. Question

What does Next.js expose about the modules and chunks it ships, and how can
Shrike use it as optional deletion evidence without becoming dependent on a
moving target?

---

## 2. What changed

Next.js 16.4 ships `next analyze export`, which writes the Turbopack module,
chunk, route, asset, and client-reference graph as JSON Lines. This turns the
build into an inspectable artifact.

The strategic value is not bundle optimization. It is **evidence**:

- a module that is present in a client chunk is definitely used;
- a module absent from every chunk is probably dead;
- a module's bytes can be attributed to routes;
- a dynamic edge the static graph missed can be recovered.

---

## 3. Record families

The exact schema is versioned. Observed families:

| Record | Fields (illustrative) | Use |
|---|---|---|
| `module` | id, path, layer, size | index, join, bytes |
| `chunk` | id, kind, route, modules, size | route membership |
| `entry` | route, runtime, chunks | route map |
| `asset` | file, size | sanity check |
| `client-reference` | from, to, kind | server actions, boundaries |

Rules:

- one JSON object per line;
- record order is not guaranteed;
- additive fields must be tolerated;
- unknown record types are skipped and counted;
- a malformed line is a diagnostic, not a crash.

**Caveat:** the shape is young. Shrike treats it as an external contract with a
version guard and a fallback, not as a stable internal API.

---

## 4. Why JSON Lines

- streamable; no full-file load;
- append-friendly;
- tolerant of partial output;
- easy to bound memory;
- easy to test with a fixture of a few lines.

A 200 MB snapshot should parse in bounded memory. The parser keeps an index, not
the raw lines.

---

## 5. Module-to-file join

Turbopack module paths can be:

- absolute;
- repo-relative;
- `turbopack://` virtual;
- loader-prefixed (`next/dist/...!`);
- query-suffixed.

Normalization:

1. strip loader prefix;
2. strip query;
3. resolve to repo root;
4. convert to `/`;
5. exact match, then longest suffix;
6. record unmatched.

Join coverage:

```text
coverage = matched / (matched + unmatched)
```

| Coverage | Status | Use |
|---|---|---|
| ≥ 0.95 | full | confirm, suppress, bytes |
| 0.80–0.95 | partial | confirm only with source proof |
| < 0.80 | low-coverage | suppress only |

A low coverage is a product signal, not a reason to guess.

---

## 6. Presence and absence

| Result | Meaning | Effect |
|---|---|---|
| Present in client chunk | Ships | Suppress deletion |
| Present in server chunk only | Server path | Weaken; source proof + server check |
| Absent, joined | Not in the build | Confirm, do not authorize |
| Unjoined | Unknown | No confirmation |
| Stale | Source mismatch | Static-only |
| Unsupported | Version | Static-only |

**Rule:** presence beats the static graph. If the build says a module ships, do
not delete it; investigate the static miss.

---

## 7. Route and byte attribution

- a route chunk maps to its route;
- a shared chunk maps to every route that loads it;
- a framework chunk is not a user route;
- client and server chunks are separate.

Report:

- per-route bytes;
- shared bytes;
- total route bytes without double counting;
- `partial` when composition is unknown.

Never report a single "bytes saved" number when chunks are shared.

---

## 8. Dynamic-edge feedback

The build can show a module is used even when the static graph has no edge.

Examples:

- a registry resolves a component by key;
- a plugin directory is loaded by a generated manifest;
- a route is created by a convention the adapter does not know.

Shrike adds a `build` edge with certainty `certain` from the nearest static
importer (or the build entry) to the module, then re-runs reachability. The
candidate is suppressed.

A build edge can only add liveness. It never proves death.

---

## 9. Server actions and client references

A `"use server"` export is an entry. Its reachability is from the client
reference that invokes it.

- If the client-reference manifest maps the action, create a certain edge.
- If the manifest is unavailable, keep the action live.
- Absence from the manifest is not proof of death; a form, a fetch, or an
  external client can call it.

A `"use client"` module is an entry only when imported or when it is a route
convention file.

A layer mismatch yields a diagnostic, not a deletion finding.

---

## 10. Version guard

| Version | Capability |
|---|---|
| 16.4+ | full |
| 16.1–16.3 | partial |
| 15.x | static-only with a warning |
| < 15 | unsupported |
| Unknown | unsupported |

If `package.json` and the snapshot disagree, the lower capability wins.

---

## 11. Fallback behavior

| Condition | Status | Behavior |
|---|---|---|
| No snapshot, no `--build` | `absent` | Static-only; explain how to produce one |
| No snapshot, `--build` | `build-failed` | Static-only; record the error |
| Snapshot stale | `stale` | Static-only; explain |
| Malformed | `malformed` | Static-only; record the line |
| Low coverage | `low-coverage` | Suppress only |
| Unsupported | `unsupported` | Static-only |

`--require-build-evidence` turns unavailable evidence into exit 4.

---

## 12. Risks

| Risk | Impact | Mitigation |
|---|---|---|
| Schema changes | Parser breaks | Version guard; additive tolerance; fixtures |
| Command renamed | Evidence gone | Static fallback; watch release notes |
| Virtual modules | Unjoined | Record; never guess |
| Multiple apps in a monorepo | Ambiguous join | Per-app snapshots (PLAN-6) |
| Low coverage | No confirmation | Suffix matching; report; no guess |
| Shared chunks | Misleading bytes | Per-route/shared separation |
| Server action absence | False positive | Conservative: absence is not death |

---

## 13. Decision impact

- Build evidence is optional, never required.
- Presence suppresses; absence confirms but never authorizes.
- Bytes are per-route and shared, never a single number.
- The receipt records status, version, hash, and coverage.
- The static product works without the build graph.

---

## 14. Open questions

| ID | Question |
|---|---|
| Q-008 | Minimum version for full evidence? |
| Q-014 | Is the schema documented per version? |
| Q-015 | How are virtual modules represented? |
| Q-016 | Does Shrike run the build itself? |
| Q-027 | Is the client-reference manifest available across versions? |
| Q-042 | How are multiple Next.js apps in a monorepo handled? |
| Q-043 | How are route groups and parallel routes attributed? |

---

## 15. Sources

See [`06-sources.md`](06-sources.md).
