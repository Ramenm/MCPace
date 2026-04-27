# Release Engineering

## Target release surfaces

1. GitHub Release platform archives for the Rust binary.
2. Thin npm package `@mcpace/cli` as a launcher/install lane.
3. Later optional package-manager surfaces such as Homebrew or WinGet.

## Rules

- GitHub Release binaries remain the source of truth.
- npm package stays a launcher surface and should not become a second core.
- do not rely on deleted shell scripts during packaging.
- provenance must be checked on the actual published package, not assumed.
- do not claim release readiness from local source checks alone.

## Current confirmed checks in this container

```bash
npm test
npm run verify:npm-pack
npm run build:release-artifacts
```

## Release automation scaffolding now in repo

- `.github/workflows/release.yml` builds a canonical source release bundle, uploads it, and smoke-checks staged host binaries on Ubuntu/Windows/macOS
- `scripts/build-release-artifacts.mjs` writes a cleaned source bundle with the archive, verification report, checksums, and `release-artifacts.json`, and syncs `reports/verification-latest.json` when it performs the proof run itself
- `scripts/generate-checksums.mjs` writes `SHA256SUMS.txt` for source or vendored-binary artifact directories
- `scripts/verify-npm-pack.mjs` asserts the npm tarball contract instead of trusting `npm pack --dry-run` output by inspection

## Still required before public release claims

```bash
# Rust build/test on a host with cargo + rustc
cargo test
cargo build --release

# runtime proof on supported hosts
./target/release/mcpace verify readiness

# later: artifact publication automation
# later: npm publish provenance validation
```

## Canonical local source bundle

Use the canonical bundle builder to create a clean source release directory in
`dist/` with one fresh archive, a matching verification snapshot, checksums, and
an artifact manifest:

```bash
npm run build:release-artifacts
```

Under the hood the bundle builder still uses `scripts/archive-release.mjs` to
create `<project-name>-v<version>-<ddmmyy-hhmmss>.zip`, but it also avoids stale
`dist/` entries from polluting the release checksums.
