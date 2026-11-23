# Code quality rules

Applies to every change in this repository.

## Both runtimes

- Keep changes scoped to one concern. Do not reformat, rename, or "clean up"
  code outside the task.
- Preserve determinism: stable iteration and sort order, no timestamps, random
  IDs, absolute host paths, or host-dependent locale in artifacts or JSON.
- Never hand-edit generated output (`packages/action/dist/main.cjs`,
  `packages/*/dist/`, platform `bin/`), or `Cargo.lock` without a dependency
  change. Regenerate with the scripts in `package.json`.
- Do not weaken a check, relax a schema, or change an invariant to make a gate
  pass. Fix the cause.

## Rust (`crates/`)

- Run `cargo fmt --all -- --check` and
  `cargo clippy --workspace --all-targets -- -D warnings` before finishing.
- Keep the crate dependency direction from `docs/architecture.md`: no crate
  depends on TypeScript. Analyzer crates read source and manifests; only the
  engine owns the snapshot cache and protocol output. Never add process spawns
  or network calls to the analyzer.
- All runtime crates use `#![forbid(unsafe_code)]`. Errors use `ShrikeError`
  (`thiserror` in core/protocol); the workspace has no `anyhow` dependency.
- serde fields are renamed explicitly. JSON is `camelCase`, Rust is
  `snake_case`. No implicit defaults that can drift from the schema.
- Do not print diagnostics with `println!` outside the engine output path;
  use `tracing`.

## TypeScript (`packages/`)

- Strict TypeScript. No `any` in public APIs; validate unknown input at the
  boundary with the project's schema tooling.
- ESM, Node 22+, package entry points only — never import another package's
  `src/` path. No circular package dependencies.
- Only `cli` writes to stdout. Commands return exit codes; libraries do not
  call `process.exit`.
- Check execution keeps explicit timeouts and the allowlisted environment.
  Never expand a check's environment to make it pass.

## Documentation

- Update the relevant page in `docs/` when behavior changes, in the same PR.
- Do not reintroduce status trackers, phase plans, or role prompts.
