# PLAN-6 — Hardening and Scale

> **Parent:** [MASTER_PROMPT.md](../MASTER_PROMPT.md) · **Tracker:** [TRACKER.md](../TRACKER.md)
> **Status:** `planned` · **Depends on:** PLAN-2 … PLAN-5 · **Unblocks:** 1.0
> **Owner prompts:** [`../prompts/04-review-agent.md`](../prompts/04-review-agent.md),
> [`../prompts/06-release-agent.md`](../prompts/06-release-agent.md),
> [`../prompts/08-adversarial-red-team-agent.md`](../prompts/08-adversarial-red-team-agent.md)

---

## 1. Goal

Take a working product loop and make it trustworthy at scale: expand the corpus,
calibrate the rules, harden security and failure paths, hit the 10k-file budgets,
and ship a signed 1.0.

Hardening is not polish. It is the phase where the claims in the README become
reproducible facts.

---

## 2. Scope

### In scope

- Corpus expansion to ≥ 250 entries.
- A labeled set of 10–20 real repositories.
- Per-rule precision and a calibration report.
- Monorepo and 10k-file scale.
- Security review and failure injection.
- Supply chain: signing, checksums, provenance, audits.
- Final docs and a release candidate.
- The 1.0 release.

### Out of scope

- New rules that are not covered by the corpus.
- Multi-language support.
- A hosted dashboard.
- A pricing/billing system.
- Telemetry in v1 (deferred).

---

## 3. Work breakdown

### 6.1 — Corpus expansion

**Task:** Grow the must-not-report corpus to ≥ 250 entries.

**Acceptance:**

- Every entry has a framework citation and an expected result.
- New categories cover parallel routes, intercepting routes, route groups,
  `use cache`, `use client` boundaries in monorepos, and generated clients.
- The corpus runs in CI and in both build-evidence modes.
- Zero findings.

**Read:** [`../docs/08-benchmarks-and-confidence.md`](../docs/08-benchmarks-and-confidence.md) §2.

**Evidence:** corpus report.

### 6.2 — Labeled set

**Task:** Assemble 10–20 public repositories with two independent labelers.

**Acceptance:**

- Repositories pinned by commit; licenses permit analysis.
- Two labelers per sample; disagreements adjudicated.
- Labels: confirmed dead, false positive, uncertain.
- The dataset version and sampling method are recorded.
- A 10% sample is re-labeled after two weeks.

**Read:** [`../docs/08-benchmarks-and-confidence.md`](../docs/08-benchmarks-and-confidence.md) §3.

**Evidence:** dataset card.

### 6.3 — Calibration report

**Task:** Publish per-rule precision with sample sizes and confidence intervals.

**Acceptance:**

- Every default rule has a precision estimate.
- Rules below 0.95 are `experimental` and blocked from auto-deletion.
- A calibration curve compares stated confidence to observed precision.
- The report links the dataset version and the exact commit.
- The numbers are reproducible from the published method.

**Read:** [`../docs/08-benchmarks-and-confidence.md`](../docs/08-benchmarks-and-confidence.md) §3.4.

**Evidence:** report in the tracker.

### 6.4 — Monorepo scale

**Task:** Validate workspace boundaries and resolution at 10k+ files.

**Acceptance:**

- pnpm, npm, and yarn workspaces are detected.
- Cross-package resolution is correct.
- A shared package used by two apps keeps its exports live.
- Deletion does not cross a package boundary without an allow.
- A 10k-file monorepo completes within the budget.

**Read:** [`../docs/03-algorithms.md`](../docs/03-algorithms.md) §7 and §9.4.

**Evidence:** benchmark report.

### 6.5 — 10k performance

**Task:** Meet every performance budget on the reference hardware.

**Acceptance:**

- 1k cold ≤ 1.5 s; warm ≤ 400 ms.
- 10k cold ≤ 6 s; warm ≤ 2 s.
- 10k peak RSS ≤ 600 MB.
- Delta 1k warm ≤ 1 s.
- Results are recorded with hardware and versions.
- A missed budget has two documented optimization attempts before any target
  change.

**Read:** [`../docs/08-benchmarks-and-confidence.md`](../docs/08-benchmarks-and-confidence.md) §4.

**Evidence:** benchmark report.

### 6.6 — Security review

**Task:** Review every trust boundary and run the security tests.

**Acceptance:**

- Analyzer executes no project code.
- Config child-process timeout is enforced.
- Verifier worktree isolation is verified.
- Redaction tests pass with synthetic secrets.
- Prompt-injection fixtures are stripped and labeled.
- Path traversal is rejected.
- Fork PR refusal is tested.
- `cargo audit` and `pnpm audit` are clean or documented.

**Read:** [`../docs/09-security-and-trust.md`](../docs/09-security-and-trust.md) §8–§9.

**Evidence:** review report; CI log.

### 6.7 — Supply chain

**Task:** Sign and checksum releases and verify provenance.

**Acceptance:**

- Binaries are built in CI and signed.
- Checksums are published and verified by the launcher.
- npm publish uses provenance.
- GitHub Actions are pinned by SHA.
- The release workflow has minimum permissions.
- A reproducible-build check is documented (or the limitation is).

**Read:** [`../docs/09-security-and-trust.md`](../docs/09-security-and-trust.md) §6.

**Evidence:** release log.

### 6.8 — Failure injection

**Task:** Test crash, timeout, corrupted cache, partial apply, and failed revert.

**Acceptance:**

- Engine crash → exit 4; no deletion.
- Timeout → exit 4; no deletion.
- Corrupted cache → discarded and re-scanned.
- Partial apply → restored; exit 3.
- Failed revert → exit 5 with a recovery path.
- Every case leaves the tree in a known state or says exactly how to restore it.

**Read:** [`../docs/05-verified-deletion.md`](../docs/05-verified-deletion.md) §8.

**Evidence:** CI log.

### 6.9 — Docs and README final

**Task:** Make every public claim reproducible and every link valid.

**Acceptance:**

- README claims link to a raw report or a doc.
- No benchmark appears in the README that the tracker cannot reproduce.
- All relative links resolve.
- Plan task IDs match the tracker.
- The architecture, algorithm, and verifier docs match the code.
- A new contributor can go from clone to first scan in one page.

**Read:** [`../scripts/check-links.mjs`](../scripts/check-links.mjs) and
[`../MASTER_PROMPT.md`](../MASTER_PROMPT.md).

**Evidence:** link check; docs review.

### 6.10 — Release candidate

**Task:** Publish `1.0.0-rc.1` to npm and GitHub.

**Acceptance:**

- Version and protocol version are correct.
- The tarball contains only allowed files.
- The standalone binaries are attached.
- The changelog is complete.
- The install path is tested on Linux, macOS, and Windows.
- A clean machine can install, scan, and open a draft PR.

**Evidence:** registry and release page.

### 6.11 — 1.0 release

**Task:** Tag, sign, and publish `1.0.0`.

**Acceptance:**

- Tag is signed.
- Binaries and checksums are attached.
- npm provenance is present.
- The release notes state the supported Next.js versions and platforms.
- The known-limitations section is honest.
- The tracker records the release with links.

**Evidence:** release URL; tracker.

### 6.12 — Opt-in telemetry

**Task:** Deferred. Do not implement in v1.

**Acceptance:**

- No telemetry code ships in 1.0.
- If revisited, it is opt-in, aggregate-only, and documented.
- A hard off switch exists before any data is collected.

**Status:** `deferred`.

---

## 4. Exit gate

- [ ] The corpus has ≥ 250 entries and zero findings.
- [ ] The labeled set has ≥ 10 repositories and a published calibration report.
- [ ] Every default rule has a precision estimate; < 0.95 rules are experimental.
- [ ] A 10k-file monorepo meets the budgets.
- [ ] All performance budgets are met or the miss is documented.
- [ ] The security review has no open high-severity finding.
- [ ] Supply chain signing, checksums, and provenance are in place.
- [ ] Failure injection passes for crash, timeout, cache, partial apply, revert.
- [ ] Docs and links are clean; plan/tracker IDs match.
- [ ] `1.0.0-rc.1` is installable on three platforms.
- [ ] `1.0.0` is tagged, signed, and published.
- [ ] No telemetry ships.

---

## 5. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Corpus expansion finds many false positives | Treat each as a blocker; fix the rule or block it |
| Labeling is subjective | Two labelers, adjudication, re-label sample |
| Precision below 0.95 on a default rule | Mark experimental; do not auto-delete |
| 10k memory blowup | Intern strings; drop ASTs; profile per phase |
| Security review finds a trust gap | Fix before release; document residual risk |
| Signing complexity delays release | Content hash is mandatory; signing is release-blocking only for binaries |
| Release claims outrun evidence | Link every claim; remove unverifiable numbers |
| Platform install fails | Test on clean machines; standalone binary fallback |

---

## 6. Definition of done

A skeptical staff engineer can read the README, click every claim, reproduce the
benchmark and the precision table, install the signed CLI on their platform, and
run a verified cleanup PR on their own repository — and the tool behaves exactly
as the receipt says it did.
