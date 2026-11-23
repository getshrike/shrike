# Git and pull request rules

- Never commit or push directly to `main`. Work in a branch, one concern per
  branch, and open a PR.
- Use Conventional Commits for titles: `feat:`, `fix:`, `docs:`, `test:`,
  `refactor:`, `perf:`, `build:`, `ci:`, `chore:` with an optional scope.
- Keep the PR description aligned with
  `.github/pull_request_template.md`: summary, changes, commands run with
  results, risk, checklist. Verification evidence lives in the PR and CI.
- Do not invent or restore a tracker. If something is blocked, state the
  blocker and the decision owner in the PR description.
- Do not include secrets, credentials, local paths, or generated files.
- Before requesting review, confirm the committed Action bundle is current
  and that `docs/` was updated where behavior changed.
- Squash noisy fixup commits before requesting review; keep the history
  readable.
