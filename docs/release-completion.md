# Release completion model

This document describes the last mile between a green source bundle and an npm release that users can install on every supported platform.

## Release lanes

MCPace publishes two npm package classes:

1. `@mcpace/cli`, the small JavaScript launcher package.
2. One native optional package per enabled target in `release-targets.json`, for example `@mcpace/cli-linux-x64-gnu`.

The main launcher is intentionally not considered publishable until every enabled native package is present as either a verified source package containing the expected binary or a verified prebuilt npm tarball in `dist/npm`, `dist`, `.artifacts/npm`, or `.artifacts`.

## Native package builder

Use this shape from a runner that has built the matching Rust target:

```sh
cargo build --release --target x86_64-unknown-linux-gnu
node scripts/build-native-npm-package.mjs \
  --target linux-x64-gnu \
  --binary target/x86_64-unknown-linux-gnu/release/mcpace \
  --out-dir dist/npm \
  --json
```

The builder refuses unknown targets, disabled targets, symlink binaries, non-regular files, non-executable Unix binaries, Windows binaries without `.exe`, and oversized binary inputs. It creates a minimal native npm package with target metadata under `package.json#mcpace`.

## Publish contract

`node scripts/verify-npm-publish-contract.mjs --enforce` checks:

- optional dependencies cover every enabled target;
- optional dependency versions match the workspace version;
- platform packages do not advertise disabled targets;
- package source metadata matches `release-targets.json`;
- prebuilt tarballs are parseable `.tgz` archives with safe paths;
- tarball `package/package.json` name, version, `mcpace.target`, `mcpace.binaryName`, `bin.mcpace`, `os`, `cpu`, and `libc` match the target;
- tarball `package/bin/<binary>` exists as a regular file and is executable for non-Windows targets;
- the publish workflow enforces the native package contract before publishing the launcher.

A tarball that merely has the right filename is not enough.

## Trusted publishing workflow

The `publish-npm` workflow builds all native target tarballs first, downloads them into `dist/npm`, enforces the publish contract, publishes the native tarballs, and only then publishes the main launcher. The workflow uses `id-token: write` for npm trusted publishing and avoids `NPM_TOKEN` / `NODE_AUTH_TOKEN` long-lived-token fallbacks.

## Remaining human gates

Before a non-dry-run release, an operator still has to verify:

- npm trusted publishers are configured on the npm side for every package name;
- the protected `npm-publish` GitHub environment requires the intended approvers;
- each runner label in `release-targets.json` is available for the repository or organization;
- `npm audit signatures` passes in CI with live registry access;
- the Rust lane has completed on all supported operating systems.
