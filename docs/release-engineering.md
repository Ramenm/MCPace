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
npm run pack:npm:dry-run
node scripts/archive-release.mjs --json --output-dir dist
```

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

## Source archive builder

Use the canonical source archive builder to create a clean zip with one meaningful
root directory and no caches/build junk:

```bash
npm run archive:release
```

The builder derives the project name from the repo manifests, keeps the current
repo version unless overridden, and emits `<project-name>-v<version>-<ddmmyy-hhmmss>.zip`.
