# 00 — Methodology

> **Parent:** [README.md](README.md) · **Status:** research method · **Date:** 2026-11-20

---

## 1. What counts as evidence

| Evidence | Use | Confidence |
|---|---|---|
| Source code at a pinned commit | Technical claims about behavior | high |
| Official documentation | API and convention claims | high |
| Release notes and changelogs | Version and feature claims | high |
| A reproduced benchmark | Performance claims | high |
| A vendor benchmark with a stated method | Directional performance claims | medium |
| A vendor survey | Market direction | medium |
| A GitHub star count or npm download count | Attention, not adoption or revenue | low–medium |
| A blog post or social post | Lead, not proof | low |
| Inference from adjacent evidence | Hypothesis | low |

A claim is stated with its evidence class. "Knip has ~86M downloads/month" is a
usage signal, not a claim that 86M developers use it. "Fallow claims 20,558 files
in 2.95 s" is a vendor benchmark until reproduced.

---

## 2. Dates

- Every research note has an authored date and an access date for each URL.
- Every competitor number has an observation date.
- Code claims name the commit or version.
- Market numbers name the report and the period.
- A number without a date is treated as stale.

---

## 3. Primary-source rule

Prefer, in order:

1. the repository at a pinned commit;
2. the official docs for the same version;
3. release notes;
4. an issue or discussion with a maintainer response;
5. a third-party analysis;
6. a social post.

If only a secondary source exists, label it `low` and do not build a decision on
it alone.

---

## 4. Reproducing a benchmark

To cite a performance number as fact:

1. identify the corpus, hardware, version, and command;
2. run it on reference hardware;
3. run 3 warmups and 5 measured runs;
4. record median, p95, and peak RSS;
5. store raw JSON;
6. link the report.

If the method is unavailable, cite the number as a vendor claim, not a fact.

---

## 5. Handling disagreement

- If two sources disagree, record both and the reason.
- If a vendor claim conflicts with the code, trust the code.
- If a doc conflicts with observed behavior, reproduce it and file a bug.
- If a design doc conflicts with research, amend the design doc or record why
  the research does not apply.

---

## 6. Research ethics

- Do not scrape private data.
- Do not use a design partner's repository without consent.
- Anonymize partner data.
- Respect licenses when quoting code.
- Do not present a competitor's bug as a market gap without evidence that users
  care.
- Do not publish a critical security repro before a fix.

---

## 7. Note template

```markdown
# <Topic>

> Status: research note · Date: YYYY-MM-DD · Confidence: high|medium|low

## Question

## Method

## Findings

| Claim | Evidence | Date | Confidence |
|---|---|---|---|

## What changed

## Decision impact

## What would change our mind

## Sources
```

---

## 8. Review

A research note is reviewed by the orchestrator before it changes a design doc.
The review checks:

- every claim has a source and a date;
- vendor claims are labeled;
- the decision impact is explicit;
- the "what would change our mind" section is real;
- the source list is updated.

Research that cannot survive this review is not merged.
