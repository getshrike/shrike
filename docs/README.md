# Development documentation

This directory is the working manual for the repository: for contributors and
for coding agents. Product-facing material lives in [`README.md`](../README.md);
short agent orientation lives in [`AGENTS.md`](../AGENTS.md).

The docs are organized by the job you are doing, not by chronology:

| If you are… | Read |
| --- | --- |
| setting up the repo or looking for a command | [development.md](development.md) |
| learning what Shrike does and what it reports | [features.md](features.md) |
| learning the algorithms and invariants | [how-it-works.md](how-it-works.md) |
| deciding where a change belongs | [architecture.md](architecture.md) |
| adding or running tests | [testing.md](testing.md) |
| opening or reviewing a PR | [contributing.md](contributing.md) |
| preparing a release | [release.md](release.md) |

## Conventions for these docs

- Each doc answers one question and links outward instead of duplicating
  content. Keep `AGENTS.md` thin; put detail here.
- Prefer commands with their exact script name over prose descriptions.
- Document current behavior only. When behavior changes, update the relevant
  doc in the same PR; never restore a status tracker or phase plan here.
- Relative links are checked by `node scripts/check-links.mjs`; the required
  doc set is checked by `node scripts/check-docs.mjs`.
