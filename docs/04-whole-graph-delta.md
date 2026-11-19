# 04 — Whole-Graph Delta

> **Parent:** [MASTER_PROMPT.md](../MASTER_PROMPT.md) · **Tracker:** [TRACKER.md](../TRACKER.md)
> **Status:** canonical · Authored 2026-11-20

---

## 1. The problem with changed-file audits

Every PR-quality tool converges on the same design: build the graph, then report
findings only in files the PR touched. Knip's CI guide, Fallow's `audit`, and
most review bots do this. The reason is signal-to-noise: a repository with years
of debt would otherwise fail every PR.

The design has a blind spot that is exactly the size of the product:

> A PR can make code dead **without touching it**.

Example:

```ts
// src/features/invoice/InvoiceCard.tsx  (touched)
- import { formatInvoice } from '../../lib/format';
- <span>{formatInvoice(invoice)}</span>
+ <span>{invoice.totalFormatted}</span>
```

```ts
// src/lib/format.ts  (untouched)
export function formatInvoice(invoice: Invoice) { ... }   // now dead
```

```ts
// src/lib/currency.ts  (untouched)
export function formatCents(cents: number) { ... }       // now dead, exclusive to formatInvoice
```

The PR review sees two changed files. Every changed-file audit reports nothing.
The repository now carries dead code that no one will notice until a future
cleanup. Multiply this by every feature removal, rename, and refactor, and the
compounding tax is what GitClear measures.

The whole-graph orphan delta reports the dead code **wherever it is**, and names
the change that orphaned it. That is the signal no incumbent produces.

---

## 2. Definition

Given a base commit `B` and a head commit `H`:

1. Build `G_B`, the graph snapshot at `B`.
2. Build `G_H`, the graph snapshot at `H`.
3. Compute the live set `L_B` in `G_B` and `L_H` in `G_H` under the same
   abstraction and entry configuration.
4. The **orphan delta** is `L_B \ L_H`: symbols and files that were live at the
   base and are not live at the head.
5. Attribute each delta item to the changed symbol(s) or file(s) that removed
   its last witness.
6. Compute the cascade closure and batch plan for the delta.

The delta is **not** "all dead code at head." It is the **newly** dead code
caused by the change. Pre-existing debt is out of scope unless the user runs a
full scan.

### 2.1 What counts as "newly dead"

A symbol is in the delta if:

- it was live at base;
- it is not live at head;
- it is not an entry at head;
- it is not public API at head;
- its file parsed successfully at both base and head;
- the snapshots are comparable.

A file is in the delta if it contained at least one delta symbol and no live
symbol remains, or if the file itself was removed at head and all its exports
were delta symbols.

---

## 3. Snapshots

### 3.1 Base snapshot

The base snapshot is built from the merge base, not from the working tree:

```bash
git merge-base origin/main HEAD
```

Options, in order of preference:

1. a cached snapshot keyed by
   `(commit, configHash, resolverHash, toolVersion, protocolVersion)`;
2. a clean `git worktree add --detach <merge-base>` and a normal scan;
3. `git archive` into a temp directory, if worktrees are unavailable;
4. fail closed. Do not approximate the base with the current tree.

The base snapshot is read-only. It is never verified or mutated.

### 3.2 Head snapshot

The head snapshot is built from the PR head commit, optionally with uncommitted
changes for the local hook path. The head snapshot carries a `dirty` flag; a
dirty head is allowed for local feedback but not for a CI receipt.

### 3.3 Comparability

Two snapshots are comparable only if:

- the same config hash;
- the same resolver hash;
- the same entry adapter version;
- the same tool and protocol version;
- the same workspace layout.

If any differs, the delta is `incomparable`, and the tool explains which input
changed. It does not silently produce a misleading diff.

Examples of incomparability:

- a PR changes `tsconfig.json` paths;
- a PR changes `shrike.config.ts` entry policy;
- the base and head use different Shrike versions;
- a workspace package is added or removed.

In these cases, the tool can still run a **full scan at head** and report
current-state dead code, clearly labeled as not a delta.

---

## 4. Reachability with provenance

### 4.1 Live set

Start from all entries:

- Next.js routes and convention files;
- server actions;
- client entry points;
- Pages Router pages and API routes;
- proxy/middleware/instrumentation;
- metadata files;
- package entry points;
- test and config entries, per policy.

BFS over the over-approximate graph. A symbol is live if it is reachable from
any entry.

### 4.2 Witness

Every live symbol stores a shortest witness path:

```text
Witness {
  entry: EntryId,
  path: Vec<(SymbolId, EdgeId)>,
  length: u32
}
```

The witness is the explanation. For a delta item, the tool shows the witness that
existed at the base and the edge that disappeared at the head.

### 4.3 Proof of death

For a dead symbol, the proof is the absence of a path under the abstraction, plus
the list of edges that were considered and rejected. The proof is not a formal
theorem; it is an auditable trace. The receipt and the report must say which
abstraction was used and which `unknown` edges were treated as blocking.

### 4.4 Why witnesses matter

A list of dead symbols is not actionable. A reviewer needs to know:

- what kept this alive before;
- what changed;
- why the tool believes nothing else keeps it alive now.

The witness answers the first two. The proof trace answers the third. Together
they turn a finding into an explanation.

---

## 5. Edge certainty

| Kind | Example | Certainty |
|---|---|---|
| Static import | `import { x } from './x'` | `certain` |
| Re-export | `export { x } from './x'` | `certain` |
| Namespace access | `import * as ns; ns.x` | `certain` if the property is static |
| JSX | `<Button />` | `certain` |
| Static dynamic import | `import('./x')` | `certain` |
| Static require | `require('./x')` | `certain` |
| Template with static prefix | ``import(`./locales/${lang}.json`)`` | `probable` + glob |
| String-keyed access | `registry[key]` | `probable` |
| Computed access | `obj[expr]` | `unknown` |
| Unresolved dynamic | `import(someVar)` | `unknown` |
| Framework convention | `page.tsx`, `route.ts`, `"use server"` | `certain` |

An `unknown` edge blocks deletion of every symbol in its bounded candidate set.
A `probable` edge blocks deletion unless the candidate is in the same SCC and has
no external referrer.

Full dynamic-scope rules: [03-algorithms.md](03-algorithms.md) §11.

---

## 6. Trigger attribution

For each delta item, the tool reports the **lost witness** and the **trigger**.

### 6.1 Algorithm

```text
witness = witnessIn(base, symbol)
edgeDiff = diffEdges(base, head)
for (symbol, edge) in witness.path:
    if edge is missing or changed in head:
        trigger = ownerOfChangedEdge(edgeDiff, edge)
        return (witness, trigger)
return (witness, indirectTrigger(closestChangedAncestor(witness, edgeDiff)))
```

### 6.2 Trigger kinds

| Kind | Meaning |
|---|---|
| `removed-import` | The changed file removed an import of the symbol |
| `removed-call` | The changed file removed the only call |
| `removed-jsx` | The changed file removed the only JSX use |
| `removed-route` | The route file was deleted or renamed |
| `renamed-export` | The export was renamed; the old name is dead |
| `removed-re-export` | A barrel stopped re-exporting the symbol |
| `changed-dynamic` | A dynamic edge changed and no longer reaches the symbol |
| `indirect` | No single edge explains it; the closest changed ancestor is named |

### 6.3 What not to claim

Do not infer intent. "This PR removed the invoice feature" is a human
interpretation. The tool reports "this changed edge removed the last witness."
That is a fact about the graph.

---

## 7. Output

```json
{
  "schemaVersion": "1.0",
  "base": { "ref": "origin/main", "commit": "abc123" },
  "head": { "commit": "def456", "dirty": false },
  "newlyDead": [
    {
      "symbol": "src/lib/format.ts#formatInvoice",
      "kind": "export",
      "file": "src/lib/format.ts",
      "lastWitness": [
        "app/invoices/page.tsx#default",
        "src/features/invoice/InvoiceCard.tsx#InvoiceCard",
        "src/lib/format.ts#formatInvoice"
      ],
      "trigger": {
        "file": "src/features/invoice/InvoiceCard.tsx",
        "kind": "removed-import",
        "change": "removed import of formatInvoice",
        "certainty": "certain"
      },
      "confidence": "certain",
      "estimatedLines": 18
    }
  ],
  "newlyDeadFiles": ["src/lib/currency.ts"],
  "cascade": {
    "symbols": 37,
    "files": 12,
    "lines": 1240
  },
  "batches": [
    { "id": "batch_1", "symbols": 22, "files": 6, "lines": 812 }
  ],
  "incomparable": null
}
```

The report must include the witness that was lost. "This was live through this
path, and this PR removed the edge" is the explanation a reviewer needs.

Full schema: [`../schemas/delta.schema.json`](../schemas/delta.schema.json).

---

## 8. Algorithm

```text
delta(base, head):
  Gb = snapshot(base)
  Gh = snapshot(head)
  assert comparable(Gb, Gh)

  Lb = reachable(Gb, entries(Gb))
  Lh = reachable(Gh, entries(Gh))

  orphaned = Lb - Lh
  for each symbol in orphaned:
      witness = witnessIn(Gb, symbol)
      trigger = attribute(witness, diffEdges(Gb, Gh))
      record(symbol, witness, trigger, certainty, estimatedLines)

  sccs = tarjan(closureCandidates)
  closure = cascade(sccs, Gh)
  batches = partition(closure)
  return delta(orphaned, closure, batches)
```

Complexity is linear in the graph for reachability and the delta, plus the cost
of the two snapshots. The delta itself is cheap once the snapshots are warm.

---

## 9. Base acquisition and caching

### 9.1 Cache key

```text
snapshotKey = blake3(
  commit,
  sortedFileHashes,
  configHash,
  resolverHash,
  toolVersion,
  protocolVersion
)
```

### 9.2 Worktree

```bash
git worktree add --detach .shrike/worktrees/base-<commit> <commit>
shrike scan --root .shrike/worktrees/base-<commit>
git worktree remove .shrike/worktrees/base-<commit>
```

The worktree is removed after the snapshot is built. If the process crashes, the
next run cleans stale worktrees under `.shrike/worktrees/`.

### 9.3 Archive fallback

If worktrees are unavailable (bare repo, restricted CI), use `git archive` into a
temp directory. If that fails, fail closed.

### 9.4 Dirty head

For the local hook, the head snapshot is the working tree with a `dirty: true`
flag. A dirty head is never used for a CI receipt or a merged PR.

---

## 10. Edge cases

| Case | Behavior |
|---|---|
| Base and head have different config | `incomparable`; explain the changed input |
| Base snapshot cannot be built | Fail closed; do not approximate |
| Head is dirty | Allowed for local; marked `dirty`; no CI receipt |
| A file is deleted at head | Its exports are delta candidates; its imports are removed |
| A file is added at head | No delta from the file itself; it can be an entry or a new live path |
| A symbol is renamed | Old symbol is delta; new symbol is live; trigger is `renamed-export` |
| A barrel removes a re-export | Delta includes the underlying symbol if no other path keeps it live |
| A cycle spans base and head | SCC condensation prevents partial deletion |
| An unknown edge appears at head | It blocks the candidate set; the delta reports suppression |
| Pre-existing dead code | Excluded from the delta; available via `scan` |
| A generated file is deleted | Excluded by policy; recorded |
| A public API export is orphaned | Blocked; reported as public API, not deleted |
| A test-only entry is removed | Test policy decides whether its dependencies are delta |

---

## 11. Fixtures

| Fixture | What it proves |
|---|---|
| `delta-removed-caller` | A removed import in a touched file orphans a helper in an untouched file |
| `delta-removed-route` | Deleting a route orphans its page-only components |
| `delta-renamed-export` | A rename orphans the old export and its exclusive deps |
| `delta-barrel` | A barrel re-export removal cascades correctly |
| `delta-cycle` | A cycle is treated as one unit |
| `delta-dynamic` | An unknown dynamic edge blocks deletion |
| `delta-monorepo` | Workspace boundaries are respected |
| `delta-no-change` | A no-op PR produces an empty delta |
| `delta-preexisting` | Pre-existing dead code is not reported as new |

Additional fixtures added in PLAN-2:

| Fixture | What it proves |
|---|---|
| `delta-generated` | Generated files are excluded and reported |
| `delta-public-api` | Public package exports are never deleted |
| `delta-test-policy` | Test-only entries are handled per policy |
| `delta-interaction` | Two changes interact to orphan a symbol; attribution is `indirect` |
| `delta-dirty` | Dirty head is marked and refused for CI receipts |

---

## 12. Acceptance criteria

- [ ] A removed caller in a touched file produces the orphaned helper and its
      exclusive dependencies in the delta.
- [ ] The lost witness and the triggering change are present in the output.
- [ ] A cycle is never partially deleted.
- [ ] An `unknown` edge blocks the candidate set and is named in the report.
- [ ] Pre-existing dead code is excluded from a PR delta.
- [ ] A no-op PR produces an empty delta.
- [ ] Incomparable inputs produce a clear error, not a wrong delta.
- [ ] A dirty head is marked and cannot produce a CI receipt.
- [ ] The same input produces byte-identical output across runs.
- [ ] The must-not-report corpus produces zero delta findings.
- [ ] A monorepo fixture respects workspace boundaries.
- [ ] A public API export is blocked even when no internal caller exists.

---

## 13. Why this is hard to copy

- It requires **two snapshots**, not one scan.
- It requires **stable IDs** across commits, or the diff is meaningless.
- It requires **witnesses** at base, not just live/dead booleans.
- It requires **edge-level diffs** to attribute the trigger.
- It requires **SCC-first closure** to avoid unstable plans.
- It requires a **verifier** to act on the result.

A changed-file audit can add a `--base` flag, but it cannot produce this output
without rebuilding its graph architecture.

---

## 14. Open questions

| ID | Question | Owner |
|---|---|---|
| Q-002 | Worktree by default, or in-place with snapshots? | PLAN-3 |
| Q-010 | How are monorepo workspace boundaries represented? | PLAN-2 |
| Q-012 | Should test-only and Storybook entries count by default? | PLAN-2 |
| Q-013 | How is a public API detected outside `package.json` exports? | PLAN-2 |
| Q-025 | How is a feature removal distinguished from a rename? | PLAN-2 |
| Q-038 | How are lockfile-only or dependency-only PRs handled? | PLAN-2 |
| Q-039 | Should the delta include deleted files with no exports? | PLAN-2 |
