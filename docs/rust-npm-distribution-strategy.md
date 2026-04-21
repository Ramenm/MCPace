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
- manual binary resolution through dev paths or an explicit env override.

### Phase 2

- optional platform packages such as `@mcpace/cli-linux-x64-gnu`;
- `@mcpace/cli` resolves the right package when present.

### Phase 3

- trusted npm publishing from CI, once release proof exists and npm/GitHub support is verified on the real pipeline.

## Non-goals

- postinstall downloaders;
- TypeScript build chain before the launcher actually needs compilation;
- npm as a substitute for GitHub Release binaries.
