# Release completion model

This document describes the last mile between a green source bundle and releases that users can install on every supported platform.

## Release lanes

MCPace publishes two npm package classes:

1. `@mcpace/cli`, the small JavaScript launcher package.
2. One native optional package per enabled target in `release-targets.json`, for example `@mcpace/cli-linux-x64-gnu`.

The main launcher is intentionally not considered publishable until every enabled native package is present as either a verified source package containing the expected binary or a verified prebuilt npm tarball in `dist/npm`, `dist`, `.artifacts/npm`, or `.artifacts`.

GitHub Releases publish a separate user-download lane. GitHub already exposes
source-code archives for every release, so the uploaded MCPace assets stay
installer-focused:

1. One native **installer** per enabled target from `scripts/build-native-installer-asset.mjs`.
2. `mcpace-v<version>-checksums.sha256`.
3. `mcpace-v<version>-release-assets.json`, a machine-readable manifest that maps platforms, target keys, hashes, installer commands, and the package-manager update policy.

Windows users install `mcpace-v<version>-win32-x64-msvc.msi` or `mcpace-v<version>-win32-arm64-msvc.msi`. The MSI installs `mcpace.exe` under Program Files and adds the install directory to the machine PATH.

Ubuntu users install `mcpace-v<version>-linux-x64-gnu.deb` or `mcpace-v<version>-linux-arm64-gnu.deb`:

```sh
sudo apt install ./mcpace-v<version>-linux-x64-gnu.deb
```

Ubuntu is a glibc distribution, so the `*-gnu` `.deb` assets are the correct Ubuntu lane. Alpine/musl assets remain in `plannedTargets` until a dedicated musl build and install proof exists.

macOS users install `mcpace-v<version>-darwin-x64.pkg` or `mcpace-v<version>-darwin-arm64.pkg` with Installer.app or:

```sh
sudo installer -pkg mcpace-v<version>-darwin-arm64.pkg -target /
```

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

## Native installer builder

Use this shape for GitHub Release installer assets:

```sh
cargo build --release --target x86_64-unknown-linux-gnu
node scripts/build-native-installer-asset.mjs \
  --target linux-x64-gnu \
  --binary target/x86_64-unknown-linux-gnu/release/mcpace \
  --out-dir dist/github \
  --json
```

Installer formats are intentionally platform-native:

- Windows: `.msi`, built from generated WiX source on Windows runners.
- Ubuntu/Debian Linux: `.deb`, built without archive extraction steps for users.
- macOS: `.pkg`, built with `pkgbuild` on macOS runners.

A ZIP or tarball that merely contains the binary is not enough for the GitHub user-download lane.
A tarball that merely has the right filename is not enough for the npm publish lane.

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

The `publish-npm` workflow builds all native target tarballs first, downloads them into `dist/npm`, enforces the publish contract, publishes the native tarballs, and only then publishes the main launcher. `dev` branch pushes publish unique prerelease versions like `0.7.8-dev.<run_number>` to the `dev` dist-tag. `main`/`master` pushes publish the stable package version to `latest` only when that exact version is not already present on npm. The workflow uses `id-token: write` for npm trusted publishing and intentionally does not set `NODE_AUTH_TOKEN`; an empty or stale token env var can prevent npm from using OIDC.

The `release-artifacts` workflow still builds and verifies the source bundle as an internal release proof, but only the native installer artifacts are composed into `github-release-assets` for upload. The workflow builds native GitHub installers with the same target matrix, smokes each built binary, verifies each installer by installing and running `mcpace help`, generates artifact attestations, writes checksums and the release manifest, and optionally creates a draft GitHub Release from that installer-focused asset set. Draft release creation stays manual so an operator can verify hashes and runner provenance before publishing.

## Update model

MCPace does not silently rewrite its running binary. The supported automatic-update path remains package-manager managed through npm until signed OS package repositories or a signed self-update feed exist:

```sh
mcpace update check --source npm
npm install -g @mcpace/cli@latest
```

GitHub installers support normal manual install/upgrade by downloading a newer MSI/DEB/PKG from the next release. They are not advertised as silent auto-updaters. The release asset manifest marks direct GitHub installers as `manual-upgrade-only` and names the future OS-native update channels that would be acceptable for automatic upgrades:

- Linux/Ubuntu: a signed apt repository for `.deb` upgrades, not just a lone downloadable `.deb`;
- Windows: a signed MSI and winget package metadata;
- macOS: a signed and notarized PKG and Homebrew cask metadata.

## Production signing and trust gates

The generated installers are installable package formats, but production public releases still need platform trust controls that require external credentials:

- Windows MSI: Authenticode-sign both the embedded `mcpace.exe` and the final MSI, then timestamp both signatures. Unsigned MSI files can install, but Windows reputation and SmartScreen behavior will be worse. Microsoft Artifact Signing / Trusted Signing through GitHub Actions OIDC is the preferred path; if that is unavailable, use an equivalent HSM-backed public code-signing certificate rather than a raw PFX in CI.
- macOS PKG: sign the CLI binary with Developer ID Application, sign the package with Developer ID Installer, notarize with Apple, and staple the notarization ticket before publishing.
- Linux/Ubuntu DEB: direct `.deb` downloads are manual-install assets; automatic apt upgrades require a signed apt repository with signed repository metadata.
- GitHub Actions: the workflow policy currently warns on tag-pinned third-party actions. For stricter supply-chain posture, pin third-party actions to full-length commit SHAs after recording the update process.
- Third-party notices: the source tree has local compatibility crates with MIT / MIT-or-Apache license declarations. Before public binary distribution, confirm whether they contain upstream-derived code and include the required third-party notices if needed.

The detailed signing setup, required GitHub variables/secrets, and go/no-go checks live in `docs/signing-and-notarization.md`. The important release invariant is that checksums, release manifests, and attestations must be generated from the final signed/stapled assets, not from pre-signing files.

## License posture

The repository declares Apache-2.0 in `LICENSE`, `Cargo.toml`, and the npm package metadata. The confirmed copyright owner text is `Copyright 2026 Ramenm`, recorded in `NOTICE` and the npm package notice. Apache-2.0 stays the recommended default for MCPace because it is permissive and includes an explicit patent grant. MIT is simpler but has no explicit patent grant; GPL/AGPL should only be chosen if the owner intentionally wants copyleft obligations. Contact details are not required in the license notice; GitHub issues and `SECURITY.md` remain the contact surfaces.

## Ubuntu compatibility target

The Linux `.deb` and npm native package lanes run on Ubuntu 24.04 runners but build `linux-*-gnu` binaries inside an `ubuntu:22.04` container. That keeps the glibc floor lower than the host runner and is intended to support Ubuntu 22.04+ and newer glibc distributions. Keep an install proof for the oldest supported glibc distribution before publishing a new Linux native npm package or `.deb`.

## Remaining human gates

Before a non-dry-run release, an operator still has to verify:

- npm trusted publishers are configured on the npm side for every package name;
- the protected `npm-publish` GitHub environment requires the intended approvers;
- each runner label in `release-targets.json` is available for the repository or organization;
- Docker is available on Linux runners for the `ubuntu:22.04` glibc baseline build;
- WiX installs successfully on Windows runners and `pkgbuild` is available on macOS runners;
- the Windows MSI is Authenticode-signed and timestamped when production signing secrets are available;
- the macOS PKG is Developer ID signed, notarized, and stapled when Apple signing credentials are available;
- Linux automatic updates are only advertised after a signed apt repository exists;
- third-party license notices are complete for any upstream-derived compatibility code;
- `npm audit signatures` passes in CI with live registry access;
- the Rust lane has completed on all supported operating systems.
