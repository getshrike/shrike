# `.agent/` — repository guidance for coding agents

This directory is the single source of truth for agent-specific behavior. It is
kept separate from `AGENTS.md` (short orientation) and `docs/` (human/agent
working manual) so the always-on context stays small.

```text
.agent/
├── rules/                  # always-on, short, one concern per file
│   ├── code-quality.md
│   ├── testing.md
│   └── git-and-pr.md
└── skills/                 # on-demand workflows (Agent Skills format)
    ├── create-pr/SKILL.md
    ├── code-review/SKILL.md
    └── add-fixture/SKILL.md
```

## How it is wired

`opencode.jsonc` at the repository root registers:

- `instructions: [".agent/rules/*.md"]` — loaded on every task, alongside
  `AGENTS.md`;
- `skills: [".agent/skills"]` — discovered by ID and loaded only when a task
  calls for them.

The files are plain Markdown, so other tools can consume them directly or via
symlink into their own config directories. Do not duplicate their content into
`AGENTS.md`; link here instead.

## Contribution rules

- A rule is a short imperative constraint that is true for every change. If it
  only applies sometimes, it belongs in a skill or in `docs/`.
- A skill is a step-by-step workflow with `name` and `description` frontmatter
  (the [Agent Skills](https://agentskills.io/specification) open format). Keep
  `SKILL.md` under ~200 lines; move detail into `references/`, `scripts/`, or
  `assets/` beside it.
- When behavior or gates change, update the rule, skill, and `docs/` page in the
  same PR, then run `node scripts/check-docs.mjs`.
