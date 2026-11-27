# Setup

Shrike does not need setup to run a read-only report in a normal Node project.
Setup matters for configuration, verification, CI, and agent integrations.

## Prerequisites

- Node.js 22 or newer.
- A native engine. Resolution order: `SHRIKE_ENGINE`, a repository
  `target/{release,debug}/shrike-engine` build, the installed platform package,
  then `PATH`.
- The project is TypeScript or JavaScript. Non-TS/JS repositories are out of
  scope.

Use `npx getshrike <command>` for a zero-install run, or install the CLI with
`npm install -D getshrike` and use `shrike <command>`.

## Initialize a repository

```bash
npx getshrike init
```

This writes `shrike.config.ts` and adds `.shrike/` to `.gitignore`. An existing
config is not replaced without `--force`.

The template is:

```ts
import type { ShrikeConfig } from "getshrike/config";

const config: Partial<ShrikeConfig> = {
  entryPolicy: "local",
  testEntries: true,
  includeGenerated: false,
  batchPolicy: { maxSymbols: 50, maxFiles: 10, maxLiveGraphPercent: 5, allowOversized: false },
  checks: [],
};

export default config;
```

An empty object is valid. Defaults are applied by both the host and the engine.

## Commonly changed fields

| Field | Default | When to change it |
| --- | --- | --- |
| `entryPolicy` | `"local"` | Use `"production"` to exclude test and local-tooling entries. |
| `testEntries` | `true` | Keep test entries protected; `false` narrows the graph. |
| `includeGenerated` | `false` | Only set `true` when generated files are genuinely removable. |
| `publicApi` | `[]` | Add globs whose exports must never be deletion candidates. |
| `generated` | `[]` | Add generated-file globs. |
| `ignoreDependencies` | `[]` | Suppress known-false unused dependency findings. |
| `batchPolicy` | caps below | Adjust symbols, files, live-graph percent, and oversized overrides. |
| `checks` | `[]` | Provide explicit verification commands instead of script discovery. |
| `verification.tier` | `"standard"` | Select the default check set. |
| `verification.mode` | `"worktree"` | Use `"in-place"` only when worktrees cannot run the checks. |

Default batch policy: `maxSymbols: 50`, `maxFiles: 10`,
`maxLiveGraphPercent: 5`, `allowOversized: false`.

Config files are resolved in this order: `shrike.config.ts`,
`shrike.config.js`, `shrike.config.mjs`, `shrike.config.json`. The first one
found wins. A malformed config exits `2`.

## Checks and tiers

`fast` is typecheck plus lint, `standard` adds test, and `full` adds build.
When `checks` is empty, Shrike discovers scripts from the project's
`package.json`. When `checks` is non-empty, it replaces discovery.

Each explicit check needs a `name` matching `^[a-z][a-z0-9-]*$` and a
`command`. Optional fields are `timeoutMs` (default 600000), `required`
(default true), `reruns` (default 0), `tier` (default `"standard"`), and `cwd`
(default `"."`).

A required check that cannot run is a fail-closed condition. `--no-checks` is
rejected by cleanup and verification commands; do not add a fake check or
remove verification to make a gate pass.

## Agent skill and adapters

Preview, then install:

```bash
npx getshrike install --dry-run
npx getshrike install
```

The default output is the portable skill directory:

```text
.agents/skills/shrike/
├── SKILL.md
└── references/
    ├── commands.md
    ├── safety.md
    ├── setup.md
    └── workflows.md
```

Targets add thin pointer skills or managed blocks for Claude Code, Cursor,
OpenCode, Codex, Gemini CLI, and GitHub Copilot. Detection uses repository
markers only; the installer never runs analysis, project scripts, or network
requests.

| Flag | Effect |
| --- | --- |
| `--target <name>` | Select one or more targets; repeatable or comma-separated. |
| `--claude`, `--cursor`, `--opencode` | Shortcuts for one target. |
| `--dry-run` | Show every file action; write nothing. |
| `--force` | Replace skill files with local edits. |
| `--no-skill` | Skip the skill and adapters. |
| `--no-hook` | Skip hook installation. |
| `--scripts` / `--no-scripts` | Add or explicitly skip package scripts. |

The skill directory is installed all-or-nothing. If any existing file differs
and `--force` is absent, no skill file is written, so a new `SKILL.md` cannot
point at a stale reference. Review the preview, then rerun with `--force` to
upgrade.

Hooks are installed for selected `claude`, `cursor`, and `opencode` targets
unless `--no-hook` is passed.

## CI

```bash
npx getshrike ci install
```

The default workflow is advisory: `mode: delta`, `fail-on: never`, and
`contents: read`. It writes `.github/workflows/shrike.yml`.

```bash
npx getshrike ci install --gate
```

`--gate` makes newly dead code blocking and adds `pull-requests: write` for the
advisory PR comment. Use it only with explicit consent.

Useful options: `--dry-run` prints the YAML without writing, `--path` selects a
different workflow file under `.github/workflows/`, `--base` sets the comparison
ref, and `--max-items` caps allowed findings. An existing workflow that Shrike
does not manage is skipped with exit `2`; inspect it and add `--force` only if
replacing it is intended.

## Hooks

```bash
npx getshrike hook install --claude
npx getshrike hook install --cursor
npx getshrike hook install --opencode
npx getshrike hook uninstall --claude
```

The hook runs after the agent stops, computes the delta, runs fast checks
against a stored baseline when available, and writes a bounded
`.shrike/brief.json`. It never applies a deletion. The total budget is 60
seconds, fast checks get 40 seconds, and the delta child process is capped at 55
seconds.

## MCP

```bash
npx getshrike mcp serve
```

The server speaks JSON-RPC over stdio, one request per line. The four read-only
tools are `shrike_delta`, `shrike_plan`, `shrike_proof`, and `shrike_receipt`.
Point the client at the local CLI command. No tool deletes code or runs project
checks; `shrike_plan` writes plan artifacts under `.shrike/`.

## Troubleshooting

| Symptom | Fix |
| --- | --- |
| `shrike-engine was not found` (exit `4`) | Build the native engine, install the platform package, or set `SHRIKE_ENGINE`. |
| Base ref cannot be resolved (exit `4`) | Fetch the base or pass an existing `--base <ref>`. |
| Dirty tree (exit `2`) | Commit or stash, or pass `--allow-dirty` where the command accepts it. Worktree verification still needs a clean tree. |
| No verification checks (exit `4`) | Add project scripts or an explicit `checks` list. Do not skip verification. |
| Stale plan (exit `4`) | HEAD, the merge base, or the tree changed. Re-plan before verifying. |
| Revert failed (exit `5`) | Stop. Run the printed `recover <snapshot>` command, confirm `git status`, then re-plan. |
| Installer action failed (exit `4`) | Fix the path, permission, or symlink escape named in the detail. Do not follow a symlink outside the repository. |
| Unknown or unsupported config field (exit `2`) | Compare against the public configuration reference and remove the field. |

## Where to go next

- `npx getshrike report` to confirm analysis works.
- `npx getshrike install` to add the agent skill.
- `npx getshrike ci install` to add the advisory workflow.
- Read `references/commands.md` for exact flags.
- Read `references/safety.md` before enabling a gate or applying cleanup.
