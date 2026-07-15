# Release completion model

This document describes the last mile between a green source bundle and releases that users can install on every supported platform.

> **Current public lane: npm only.** Native `.msi`, `.deb`, and `.pkg` artifacts are private draft proofs. Do not publish or recommend them until Windows artifacts are Authenticode-signed and macOS artifacts are Developer ID signed, notarized, and stapled. Checksums, manifests, and attestations must be regenerated from those final bytes.

## Release lanes

MCPace publishes two npm package classes:

1. `@mcpace/cli`, the small JavaScript launcher package.
2. One native optional package per enabled target in `release-targets.json`, for example `@mcpace/cli-linux-x64-gnu`.

The main launcher is intentionally not considered publishable until every enabled native package is present as either a verified source package containing the expected binary or a verified prebuilt npm tarball in `dist/npm`, `dist`, `.artifacts/npm`, or `.artifacts`.

The GitHub Release workflow prepares a separate **private draft proof** lane. It is not currently a supported user-download lane. GitHub already exposes source-code archives for every release, so any future signed MCPace assets remain installer-focused:

1. One native **installer** per enabled target from `scripts/build-native-installer-asset.mjs`.
2. `mcpace-v<version>-checksums.sha256`.
3. `mcpace-v<version>-release-assets.json`, a machine-readable manifest that maps platforms, target keys, hashes, installer commands, and the package-manager update policy.

After the signing gate is implemented and proven, Windows users will install `mcpace-v<version>-win32-x64-msvc.msi` or `mcpace-v<version>-win32-arm64-msvc.msi`. The MSI installs `mcpace.exe` and `mcpace-agent-launcher.exe` under Program Files and adds the install directory to the machine PATH. The launcher is required for native Windows login-start because it starts the console CLI binary without opening a terminal window.

After the public installer lane is enabled, Ubuntu users will install `mcpace-v<version>-linux-x64-gnu.deb` or `mcpace-v<version>-linux-arm64-gnu.deb`:

```sh
sudo apt install ./mcpace-v<version>-linux-x64-gnu.deb
```

Ubuntu is a glibc distribution, so the `*-gnu` `.deb` assets are the correct Ubuntu lane. Alpine/musl assets remain in `plannedTargets` until a dedicated musl build and install proof exists.

After signing, notarization, and stapling are implemented and proven, macOS users will install `mcpace-v<version>-darwin-x64.pkg` or `mcpace-v<version>-darwin-arm64.pkg` with Installer.app or:

```sh
sudo installer -pkg mcpace-v<version>-darwin-arm64.pkg -target /
```

## Native package builder

Use this shape from a runner that has built the matching Rust target:

```sh
cargo build --release --locked --target x86_64-unknown-linux-gnu --bins
node scripts/build-native-npm-package.mjs \
  --target linux-x64-gnu \
  --binary target/x86_64-unknown-linux-gnu/release/mcpace \
  --out-dir dist/npm \
  --json
```

The builder refuses unknown targets, disabled targets, symlink binaries, non-regular files, non-executable Unix binaries, Windows binaries without `.exe`, missing required Windows sidecars, and oversized binary inputs. It creates a minimal native npm package with target metadata under `package.json#mcpace`. For Windows targets the package must include `bin/mcpace-agent-launcher.exe` beside `bin/mcpace.exe`.

## Native installer builder

Use this shape only to produce private draft proof artifacts until the signing gates above are implemented:

```sh
cargo build --release --locked --target x86_64-unknown-linux-gnu --bins
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

## Installed-runtime verification

Every release installer is tested after installation, not only unpacked or inspected. The workflow runs `scripts/installer-runtime-smoke.mjs` against the installed executable in an isolated temporary root. It requires a matching `--version` and configuration version, then verifies `init`, `up --client none` on a reserved loopback port, endpoint persistence, health, MCP `initialize`, `initialized`, `tools/list`, and clean `serve stop` teardown.

macOS jobs add target-specific checks before and after PKG installation: `file`/`lipo -archs` must report the expected `x86_64` or `arm64` Mach-O architecture, `otool -L` records linked system libraries, and `pkgutil --pkg-info io.github.ramenm.mcpace` must find the installed package receipt. This is a native runner proof for both Darwin targets; it does not replace final Developer ID signing, notarization, and physical-device validation.

The npm publish matrix also runs `scripts/native-npm-install-smoke.mjs` for every target before uploading its tarball. It installs the just-packed launcher and just-built native package into an empty temporary npm prefix with the registry deliberately unreachable, verifies that the launcher selected that exact optional dependency, runs `mcpace --version` through the launcher, and then executes the isolated runtime lifecycle. This is the standard user `npm install -g @mcpace/cli` resolution path without a globally installed MCPace masking an error.

## Publish contract

`node scripts/verify-npm-publish-contract.mjs --enforce` checks:

- optional dependencies cover every enabled target;
- optional dependency versions match the workspace version;
- platform packages do not advertise disabled targets;
- package source metadata matches `release-targets.json`;
- prebuilt tarballs are parseable `.tgz` archives with safe paths;
- tarball `package/package.json` name, version, immutable `mcpace.releaseSha`, `mcpace.target`, `mcpace.binaryName`, `bin.mcpace`, `os`, `cpu`, and `libc` match the release;
- tarball `package/bin/<binary>` exists as a regular file and is executable for non-Windows targets;
- Windows tarballs include `package/bin/mcpace-agent-launcher.exe` and `package.json#mcpace.sidecarBinaries`;
- the publish workflow enforces the native package contract before publishing the launcher.

A tarball that merely has the right filename is not enough.

## Trusted publishing workflow

The `publish-npm` workflow builds all native target tarballs first, downloads them into `dist/npm`, enforces the publish contract, publishes missing native tarballs, and publishes the launcher last. All Cargo, npm workspace, lock-file, configuration, launcher, and native-package versions must match.

Real stable publication is **tag-only**: an exact `vX.Y.Z` tag must match package metadata and the checked-out immutable SHA. `main` and `master` do not publish. The `dev` branch publishes unique `<version>-dev.<run_number>` prereleases to the `dev` dist-tag. Manual dispatch is packaging dry-run only; `version_override` is accepted only in that dry-run.

Every candidate package carries `package.json#mcpace.releaseSha`. A retry may skip an existing exact name/version only when registry metadata reports the same release SHA; missing or different SHA metadata fails closed so a partial package set cannot mix commits. Resume verifies package identity but deliberately does not mutate npm dist-tags. Before announcing completion, verify `npm view @mcpace/cli dist-tags --json`; repairing an externally changed tag is a separate, audited operator action. The protected publish job re-resolves stable tags immediately before mutation. It uses `id-token: write` for npm trusted publishing and intentionally does not set `NODE_AUTH_TOKEN`; an empty or stale token env var can prevent npm from using OIDC.

If npm rejects trusted publishing with `E404` / "could not be found or you do not have permission", configure the package-side trusted publisher entries in bulk instead of clicking each package manually:

```bash
npm login --auth-type=web
npm run npm:trust:plan
npm run npm:trust:configure
```

The bulk helper uses `npm trust github` with npm 11.18, repository `Ramenm/MCPace`, workflow `publish-npm.yml`, environment `npm-publish`, and `--allow-publish` for `@mcpace/cli` plus each enabled native optional package. The first trust command may require 2FA; use npm's temporary "skip 2FA for the next 5 minutes" option so the remaining package entries can be created automatically.

The `release-artifacts` workflow builds and verifies the source bundle as an internal release proof, while only native installer artifacts are composed into `github-release-assets`. It builds the target matrix and runs the full installed-runtime lifecycle described above, then generates attestations, checksums, and the release manifest. It can optionally create or reconcile a draft GitHub Release.

**The current draft is unsigned pre-release proof only. Do not publish it.** The workflow does not yet Authenticode-sign Windows artifacts or Developer ID sign/notarize/staple macOS artifacts. Production signing must be inserted before the native attestation step; checksums, the release index, composed-set attestation, and draft upload must then run from those final signed/stapled bytes. A digest-mismatched draft is intentionally rejected rather than overwritten, so obsolete unsigned draft assets must be removed under an explicit human cleanup procedure before the signed workflow is rerun. The npm-first package lane may be evaluated separately, but unsigned MSI/PKG assets must remain private/draft and must not be recommended to users.

## Update model

MCPace does not silently rewrite its running binary. The supported automatic-update path remains package-manager managed through npm until signed OS package repositories or a signed self-update feed exist:

```sh
mcpace --version
mcpace update check --source npm
npm install -g @mcpace/cli@latest
```

`mcpace --version` reports the compiled binary/package version. Project configuration
versions remain visible in `mcpace doctor` as `Config version`, so an installed npm
binary is not masked by a local `mcpace.config.json` or a Windows autostart root.

When the local dashboard is open, it checks this same npm metadata once and caches the result for six hours. If a newer version exists, the dashboard shows a copyable package-manager command. It never silently downloads, replaces, or restarts the running binary; set `MCPACE_UPDATE_SOURCE=none` to disable network update checks.

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
- Third-party notices: MCPace no longer ships local compatibility crates that shadow standard crates. Keep the generated third-party notice review aligned with the upstream crates resolved by Cargo.lock before public binary distribution.

The detailed signing setup, required GitHub variables/secrets, and go/no-go checks live in `docs/signing-and-notarization.md`. The important release invariant is that checksums, release manifests, and attestations must be generated from the final signed/stapled assets, not from pre-signing files. Until the workflow implements that ordering, GitHub installer release publication is blocked.

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
- the Windows executable and MSI are Authenticode-signed and timestamped before any public installer release;
- the macOS binary and PKG are Developer ID signed, and the PKG is notarized and stapled before any public installer release;
- Linux automatic updates are only advertised after a signed apt repository exists;
- third-party license notices are complete for upstream crates and packaged native tooling;
- `npm audit signatures` passes in CI with live registry access;
- the Rust lane has completed on all supported operating systems.

## Release readiness gate

`npm run check:release-ready` emits a non-blocking `mcpace.releaseReadiness.v1` report. `npm run check:release-ready:enforce` is the fail-closed form used by the release-facing CI entrypoint. The release is not fully proven until `Cargo.lock` is synchronized and reviewed, the pinned locked Rust gates pass, `npm run check:ci` exits 0, required native artifacts exist, and every external trust/signing gate above is recorded.
