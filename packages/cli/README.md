# shrike

Verified dead-code cleanup: a whole-graph PR orphan delta, verified deletion
batches, and a portable receipt.

```bash
npm install -D getshrike
npx shrike scan
npx shrike delta --base origin/main --json
```

The native engine is a Rust binary. The launcher resolves it from
`SHRIKE_ENGINE`, a repository `target/{release,debug}` build, `PATH`, or a
platform package. A missing engine fails closed with exit code 4.

Read-only first:

```bash
shrike delta --base origin/main
shrike plan --base origin/main
```

`shrike prune` verifies each batch in a worktree, reverts failed edits, writes a
receipt, and opens a draft PR when `gh` is available. It never commits to the
base branch.

See the repository README and `docs/architecture.md` for hooks, the Action, MCP,
and the trust boundary.
