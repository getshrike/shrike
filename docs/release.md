# Releasing

The release workflow is `.github/workflows/release.yml`; treat it as the source
of truth for steps and permissions.

## Versioning

Versions are semver and are set everywhere at once:

```bash
node scripts/set-version.mjs 1.2.3
```

That updates all package manifests, `packages/cli/src/version.ts`, and
`[workspace.package]` in `Cargo.toml`. Do not edit those files individually.

## Tagging and publishing

1. Land the version change and changelog update on `main` via PR.
2. Push a `vX.Y.Z` tag. A semver prerelease suffix routes npm to the `next`
   tag; a plain version routes to `latest`.
3. The `release` job builds all five native targets, packs the CLI and platform
   tarballs, attaches SHA-256 sidecars, attestations, and keyless signing, and
   drafts the GitHub release.
4. Publishing to npm is a separate, explicit `workflow_dispatch` with
   `publish: true`, so an attested artifact can be inspected before publish.

`pnpm run release:notes` drafts notes from the changelog; `pnpm run check:package`
and `node scripts/test-install.mjs` are the local tarball and clean-machine
install gates. First-time publish requires registry credentials held outside
the repository.

## After a release

- Confirm both bin names (`getshrike`, `shrike`) launch and report the version.
- Confirm the checksum sidecars and attestations are attached to the GitHub
  release.
- Update [`CHANGELOG.md`](../CHANGELOG.md) only through the release PR; do not
  rewrite history in the docs.
