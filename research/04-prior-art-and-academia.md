# 04 — Prior Art and Academia

> **Parent:** [README.md](README.md) · **Status:** research note · **Date:** 2026-11-20 · **Confidence:** high
> **Method:** literature review of dead-code analysis, program slicing, debloating, and delta debugging.

---

## 1. Question

What can Shrike learn from decades of dead-code analysis, program slicing,
software debloating, and delta debugging — and which mistakes should it avoid?

---

## 2. Dead-code analysis

### 2.1 The classic approaches

| Approach | Idea | Strength | Weakness |
|---|---|---|---|
| Reachability | A symbol is live if reachable from an entry | Sound over-approximation, explainable | Needs accurate entries and edges |
| Liveness analysis | Dataflow over a control-flow graph | Precise within a function | Not whole-program; expensive |
| Use-def / def-use | Track definitions and uses | Local precision | Cross-file is hard |
| Type-based | Unused members/props | Good for TS types | Not a graph reachability claim |
| Heuristic | Naming, file conventions | Cheap | False positives; erodes trust |

Shrike uses reachability over an over-approximate graph. It is the only approach
that scales to a whole repository and produces a witness a human can inspect.

### 2.2 Why dead-code tools fail

- **Entries are wrong.** A route, server action, or plugin is missed, and live
  code is reported dead.
- **Dynamic edges are ignored.** A string-keyed registry is invisible.
- **The report is not actionable.** A list of 400 findings with no witness is
  ignored.
- **The fix is unsafe.** A bulk delete breaks the build, and the team never runs
  the tool again.
- **There is no proof.** A JSON report does not tell a reviewer what was checked.

Shrike's design is a response to each failure: framework entries, bounded dynamic
scopes, witnesses, verified batches, and a receipt.

---

## 3. Program slicing

### 3.1 Static slicing

A program slice is the set of statements that can affect a point of interest.
Weiser's slicing and later work show how to compute dependence-based slices.

**Relevance:** a deletion candidate's "exclusive dependencies" are a backward
slice from the candidate, restricted to symbols with no other live referrer.

**Difference:** Shrike computes a forward reachability complement and then a
closure over exclusive dependencies. It does not need a full dependence graph,
but the slicing literature explains why a syntactic "unused export" is not the
same as a semantic slice.

### 3.2 Dynamic slicing

A dynamic slice uses an execution trace. It is more precise but requires running
the program with representative inputs.

**Relevance:** the project's tests are a partial dynamic check. Shrike uses them
as verification, not as the primary analysis.

**Difference:** Shrike does not instrument production. Runtime tracing is a
non-goal in v1.

---

## 4. Software debloating

Debloating removes code that is not needed for a workload. Research systems use
profiling, forced execution, or symbolic reasoning to identify removable code.

**Relevance:** dead-code removal is a conservative, source-level debloating
problem. The literature's lesson is that **the workload defines liveness**. A
generic "unused" claim is only as good as the entry model.

**Difference:** Shrike's workload is the framework's route/server-action graph
plus the project's own checks, not a sampled execution.

---

## 5. Delta debugging

### 5.1 The idea

Delta debugging (Zeller) minimizes a failing input by splitting it and testing
subsets, assuming monotonicity or handling interactions.

**Relevance:** Shrike's bisection is delta debugging over a deletion batch, with
the project's checks as the oracle.

### 5.2 What Shrike borrows

- split by a deterministic order;
- test subsets;
- stop at a minimal failing set;
- record an interaction failure when both halves pass but the whole fails.

### 5.3 What Shrike changes

- the "input" is a set of SCCs, not arbitrary lines;
- a split must respect the no-shared-file invariant;
- the oracle is a baseline comparison, not a single pass/fail;
- a timeout is a failure, not a reason to guess.

---

## 6. Change-impact analysis

Change-impact analysis asks which code can be affected by a change. Shrike's
orphan delta is the inverse: which code **lost** its last path from an entry
because of a change.

**Relevance:** the delta is a reachability diff, not a dependency diff. That is
why a changed-file audit cannot produce it.

---

## 7. Incremental and compositional analysis

- Incremental analysis reuses results for unchanged files.
- Compositional analysis summarizes a module and composes summaries.

Shrike uses both: a per-file summary and a content-addressed cache. The
literature warns that incremental analyses can be unsound if the summary omits a
fact that a later change needs. Shrike's summary must include dynamic forms and
unresolved references, not just imports and exports.

---

## 8. Lessons for the algorithm

1. **The entry model is the product.** Most false positives are missing entries.
2. **Unknown is not dead.** A conservative over-approximation is a feature.
3. **A witness is the explanation.** A boolean is not actionable.
4. **Cycles are units.** Partial-cycle deletion is unstable.
5. **Bisection is the right failure isolation.** Do not guess the culprit.
6. **A baseline is mandatory.** A test that was already failing is not a
   regression.
7. **Determinism makes the result auditable.** Same input, same output.
8. **The fix is part of the analysis.** A finding without a safe application path
   is incomplete.

---

## 9. What the literature does not solve

- Framework conventions (Next.js routes, server actions, client boundaries).
- Package export maps and monorepo resolution.
- Build-graph joins.
- The trust and review workflow.
- The economics of a cleanup PR.

Those are product problems, not analysis problems. Shrike's differentiation is
mostly there.

---

## 10. Decision impact

- Reachability with witnesses is the core.
- Dynamic scopes are explicit and bounded.
- SCC condensation precedes closure.
- Bisection is delta debugging with project checks.
- Runtime tracing is out of scope in v1.
- The verifier is as important as the analyzer.

---

## 11. Sources

See [`06-sources.md`](06-sources.md) for the reading list.
