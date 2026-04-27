# Rust + npm Distribution Strategy

## Decision

- keep Rust as the only product implementation core;
- use npm as a familiar install and update lane for JavaScript-oriented users;
- keep the npm package thin enough that it does not become a second runtime.

## Why not TypeScript as a second core

TypeScript is popular, but popularity is not enough reason to duplicate the core.
A second implementation would double parity, testing, release, and security work.

## Package topology

### Phase 1

- one root npm workspace;
- one thin launcher package: `@mcpace/cli`;
- manual binary resolution through dev paths or an explicit env override;
- optional vendored binaries under `packages/npm/cli/vendor/<target>/` for self-contained host-built artifacts without postinstall downloads.

### Phase 2

- optional platform packages such as `@mcpace/cli-linux-x64-gnu`;
- `@mcpace/cli` resolves vendored binaries first, then the right package when present.
- user-level autostart remains owned by the Rust CLI (`mcpace service ...`) so
  package managers only need to put the binary on disk and in `PATH`.

### Phase 3

- trusted npm publishing from CI, once release proof exists and npm/GitHub support is verified on the real pipeline.

### Phase 4

- Homebrew formula for macOS/Linux;
- WinGet manifest for Windows;
- Debian/Ubuntu `.deb` and optional APT repository once signing, upgrade, and
  uninstall tests exist.

## Non-goals

- postinstall downloaders;
- TypeScript build chain before the launcher actually needs compilation;
- npm as a substitute for GitHub Release binaries.
- package-manager scripts that duplicate the Rust autostart implementation.
