# Command reference

Every command accepts `--help`. Read-only commands accept `--json`; `mcp serve`
speaks JSON-RPC over stdio instead. The `getshrike` binary is equivalent to
`shrike`: both are installed by the npm package.

## Binary and command names

| Situation | Use |
| --- | --- |
| The package is not installed in the project | `npx getshrike <command>` |
| The package is a project dependency | `shrike <command>` or `npx --no-install shrike <command>` |
| Inside the Shrike repository | `node packages/cli/src/main.ts <command>` |

`npx --no-install shrike` resolves the local `node_modules/.bin/shrike` and
fails if it is absent. Prefer it in scripts that must not download anything.

Never run `npx` with the short binary name. It can resolve the unrelated npm
package `shrike` instead of this CLI.

## Command matrix

| Command | Syntax | Purpose | Mutates | `--json` |
| --- | --- | --- | --- | --- |
| `init` | `shrike init [--force]` | Create `shrike.config.ts` and ignore `.shrike/` | Config, `.gitignore` | Yes |
| `report` | `shrike report [--base <ref>] [--verbose] [--quiet] [--width <n>] [--no-tui] [--allow-dirty]` | Dead-weight report; always plain. Run `shrike` with no command for the interactive view | Cache; branch mode uses a temporary worktree | Yes |
| `scan` | `shrike scan` | Current-state evidence and candidates | Cache | Yes |
| `deps` | `shrike deps` | Unused, unlisted, and transitive dependency evidence | Cache | Yes |
| `delta` | `shrike delta --base <ref> [--fail-on newly-dead\|never]` | Whole-graph PR orphan delta | Cache; branch worktree | Yes |
| `plan` | `shrike plan --base <ref>` | Deletion batches, impact, blast radius | Cache and `.shrike/` plan artifacts | Yes |
| `clean` | `shrike clean [--base <ref>] [--dry-run] [--in-place] [--allow-dirty] [--pr] [--tier <tier>] [--build <path>] [--require-build-evidence]` | Apply, verify, and commit branch-orphaned batches | Worktree, branch commits, receipts, recovery snapshots | Yes |
| `clean --all` | `shrike clean --all [--dry-run] [--in-place] [--allow-dirty] [--pr] [--tier <tier>] [--build <path>] [--require-build-evidence]` | Current-state cleanup through the same pipeline | Same as `clean`; rejects `--base` | Yes |
| `prune` | `shrike prune --base <ref> [same options as clean]` | Original name for branch cleanup | Same as `clean` | Yes |
| `why` | `shrike why <path#Name> [--json]` | Human proof for one symbol | Cache | Yes |
| `verify` | `shrike verify [--plan <id> \| <receipt-id>] [--in-place] [--allow-dirty] [--tier <tier>] [--quiet]` | Re-run a saved plan, or inspect a receipt by ID | Worktree and receipts | Yes |
| `receipt` | `shrike receipt [id]` | List receipts or show one | No | Yes |
| `recover` | `shrike recover <snapshot>` | Restore files from a recovery snapshot | Files | Yes |
| `proof` | `shrike proof <path#Name>` | Witness path or proof trace | Cache | Yes |
| `evidence` | `shrike evidence [--build <path>] [--require-build-evidence] [--run-build] [--build-command <cmd>] [--no-client-boundary-leaks]` | Join a Next.js build-evidence export | Cache | Yes |
| `install` | `shrike install [--target <name>]... [--dry-run] [--force] [--no-skill] [--no-hook] [--no-scripts] [--scripts] [--claude] [--cursor] [--opencode]` | Write the portable agent skill, adapters, and optional hook | Agent config files, optional `package.json` scripts | Yes |
| `ci install` | `shrike ci install [--gate] [--dry-run] [--force] [--path <workflow-path>] [--base <ref>] [--max-items <n>]` | Write the GitHub Actions workflow | `.github/workflows/` | Yes |
| `hook install` | `shrike hook install --claude\|--cursor\|--opencode` | Install a client hook directly | Client config | Yes |
| `hook uninstall` | `shrike hook uninstall --claude\|--cursor\|--opencode` | Remove only Shrike-managed hook entries | Client config | Yes |
| `hook run` | `shrike hook run [--quiet]` | Compute delta and write a bounded brief | `.shrike/brief.json`, cache | Yes |
| `mcp serve` | `shrike mcp serve` | Read-only MCP server over stdio | No | JSON-RPC |
| `cache clean` | `shrike cache clean` | Delete the configured cache directory | Cache | Yes |
| `cache verify` | `shrike cache verify` | Check cached snapshots for the expected schema version | No | Yes |
| `version` | `shrike version` | Host version, protocol version, engine name | No | Yes |

## Scope rules

- `report`, `scan`, `deps`, `delta`, `plan`, `why`, `proof`, `receipt`, `cache
  verify`, and `version` are read-only with respect to project source.
- Branch scope compares the merge base with the working tree. `report --base`,
  `delta`, `plan`, and dry-run cleanup create and remove a temporary worktree
  under `.shrike/worktrees/`.
- `delta`, `plan`, `clean`, and `prune` accept `--base <ref>`. When omitted,
  Shrike tries `origin/main`, `main`, `origin/master`, then `master`.
- `clean --all` has no branch base. It rejects `--base` with exit `2`.
- `prune` is the original name for `clean` without `--all`. Both share the same
  worktree, baseline, checks, bisection, revert, receipt, and `--pr` machinery.
- Neither cleanup path commits to the base branch.

## Common flags

| Flag | Effect |
| --- | --- |
| `--json` | Machine-readable output on stdout. Exit codes do not change. |
| `--base <ref>` | Compare against this git ref. Branch commands only. |
| `--dry-run` | Preview actions or batches; write no project source. |
| `--force` | Replace a managed file with local edits, or an unmanaged CI workflow. |
| `--in-place` | Apply or verify in the working tree instead of a worktree. Requires a clean tree unless `--allow-dirty` is also set. |
| `--allow-dirty` | Permit analysis or cleanup on a dirty tree. Worktree verification still requires a clean tree. |
| `--tier fast\|standard\|full` | Select the check set. `fast` is typecheck plus lint, `standard` adds test, `full` adds build. |
| `--build <path>` | Join a Next.js build-evidence export. |
| `--require-build-evidence` | Fail closed when the evidence is missing or unusable. |
| `--pr` | Open a draft PR with `gh` after a verified cleanup. |
| `--fail-on newly-dead\|never` | `delta` policy. `never` exits `0` even with findings. |
| `--max-items <n>` | `ci install` failure cap for newly dead items. `0` means no cap. |
| `--no-checks` | Rejected. A verified deletion requires at least one project check. |

## Exit codes

| Exit | Meaning |
| --- | --- |
| `0` | Clean, or the operation completed. |
| `1` | Findings, or a dry run found batches. This is data, not necessarily a tool failure. |
| `2` | Usage or configuration error, including `--all` with `--base`. |
| `3` | Verification failed; changes were reverted. |
| `4` | Could not run; Shrike failed closed. |
| `5` | Revert failed; manual recovery is required. |

Per-command nuances:

- `report`, `scan`, `deps`, `delta`, and `plan` return `1` when they find
  something. `delta --fail-on never` returns `0`.
- `why` returns `0` whether the symbol is live or dead. Read the proof.
- `clean` and `clean --all` return `1` only for a dry run. A real run returns
  `3` or `4` on failure.
- `verify <receipt-id>` returns `0` only when the receipt outcome is
  `verified`; any other outcome returns `3`.
- `install` returns `4` when any action fails. Skipped files with local edits
  are not failures.
- `ci install` returns `2` when an unmanaged workflow exists and `--force` was
  not passed.
- `recover` returns `4` for a missing or invalid snapshot, and `5` when the
  restore itself fails.

## JSON output

`--json` is a stable machine form. Parse it; do not paste it wholesale into the
conversation. A non-zero exit code still means the output may contain useful
data, especially for findings. When an error occurs, the CLI writes a JSON error
object to stderr and sets the matching exit code.

Useful shapes:

- `report --json` is the cleanup report: totals, groups, protected items,
  diagnostics, dependencies, and suggested next commands.
- `delta --json` is the delta report: newly dead symbols and files, cascade
  counts, and comparison metadata.
- `plan --json` is the deletion plan: batches, blocked items, blast radius, and
  impact.
- `why --json` and `proof --json` are the engine proof object: symbol, liveness,
  witness, and trace.
- `receipt --json` is the receipt list or one receipt.
- `deps --json` is the dependency report grouped by package.

## Output selection

- Human mode is the default and never prompts for input, so it is safe to pipe.
- `--json` writes one machine document to stdout; errors go to stderr.
- `report` is always plain. Run `shrike` with no command for the interactive
  view; `--no-tui` disables it, and `--quiet` keeps only the headline.
- `--verbose` adds per-item detail to `report`; the other commands list their
  items by default.

## Output examples

```console
$ npx getshrike report
Shrike report · current state · 9f2c1ab

  Dead weight  4.8%
  1,284 removable lines / 26,900 analyzed source lines

  38 files · 61 symbols (58 exports) · 4 dependencies (advisory)
  12 protected · 7 diagnostics (2 actionable)

  Next: npx getshrike clean --all
        npx getshrike report --verbose
```

```console
$ npx getshrike delta --base origin/main
Shrike delta: 2 newly dead symbols in 1 file
  src/format.ts#formatInvoice  -18 lines  [src/format.ts]
  src/format.ts#formatCurrency  -7 lines  [src/format.ts]
Cascade: 0 symbols, 0 files, 0 lines
```

```console
$ npx getshrike plan --base origin/main
Plan plan_<id>: 2 symbols, 1 files, 25 lines
  batch_1: 2 symbols, 1 files, 25 lines
```

```console
$ npx getshrike why src/format.ts#formatInvoice
src/format.ts#formatInvoice
  file: src/format.ts
  kind: function
  lines: 18-36
  dead at HEAD
  no witness from any entry.
```

## Where to go deeper

- [Exit codes](https://getshrike.dev/reference/exit-codes)
- [Report format](https://getshrike.dev/reference/report-format)
- [Configuration](https://getshrike.dev/reference/configuration)
- [Verified pruning](https://getshrike.dev/guides/verified-pruning)
- [Agent integrations](https://getshrike.dev/guides/agent-integrations)
