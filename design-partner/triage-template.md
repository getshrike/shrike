# Shrike trial triage

Copy this file for each finding that is wrong, surprising, or blocked. Remove
anything confidential before sharing.

## Trial context

- Date:
- Repository (or redacted name):
- Commit / branch:
- Base ref:
- Shrike version:
- Engine path:
- Build evidence used? (none / path / version / status):

## Finding

- Symbol:
- File:
- Kind: newly dead / pre-existing / blocked / diagnostic
- Plan batch (if any):
- Witness or proof shown by Shrike:

## Expected result

- Why do you believe the symbol is still live?
- Is it reachable dynamically, through a framework convention, through a
  generated file, through a public API, or through a test?
- Is the issue in the graph, the entry adapters, the build join, or the plan?

## Reproduction

- Smallest file(s) and command:
- Check command and result:
- If build evidence is involved: export version, route, and layer.

## Classification

- [ ] False positive (Shrike reported a live symbol as dead)
- [ ] False negative (Shrike missed a dead symbol)
- [ ] Unsafe plan (batch would delete something live)
- [ ] Blocked by a missing check or configuration
- [ ] Usability / unclear output
- [ ] Other:

## Corpus candidate

- [ ] Yes — add to `fixtures/must-not-report/` with a category and reason.
- [ ] No — explain why the case is project-specific.

## Notes

-
