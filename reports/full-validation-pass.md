# Full validation pass

This pass re-ran the project from a clean source bundle and checked Node/package install behavior, release artifact creation, local load-test prerequisites, static Rust hygiene, and Windows-sensitive launcher paths.

## Environment inventory

- OS: Debian GNU/Linux 13 (trixie), Linux x86_64.
- Node.js: 22.16.0.
- npm: 10.9.2.
- Rust toolchain: not available in this sandbox (`cargo`, `rustc`, and `rustup` were not installed).
- Archive tools: `zip` and `unzip` are present, but the project release builder uses the Node-only ZIP writer.

## Additional fixes in this pass

- `npm run load:local` now honors the same explicit binary environment contract as the npm launcher: `MCPACE_BINARY_PATH` and `MCPACE_DEV_BINARY`, while still accepting the script-specific `MCPACE_BINARY`.
- The load-test script now rejects missing, directory, and non-executable binary paths with clearer messages and reports both default candidate paths.
- The npm launcher no longer treats an arbitrary consumer project's `target/release/mcpace` or `dist/mcpace` as a development MCPace binary. Local dev binaries are considered only when the resolved root is the MCPace source workspace.
- `serve` resource forwarding now uses the centralized `resources::append_serve_resource_args` helper instead of keeping a separate local copy.
- Added regression coverage for load-test env alignment, accidental consumer-project target binaries, and the retired local `serve_resource_args` duplicate.

## Commands run

Passed:

```bash
npm install --ignore-scripts
npm run check
npm run pack:npm:dry-run
npm run release:dry-run
npm run build:release-artifacts
node scripts/load-test-local.mjs --help
```

Expected blocked checks in this sandbox:

```bash
npm run build
npm run test:rust
cargo fmt --check
cargo test
npm run load:local -- --duration-ms 100 --concurrency 1
```

Reason: a Rust-capable host is still required. The local load test now fails with an explicit missing-binary message instead of pointing only at `target/debug` or ignoring the standard MCPace binary env vars.

## Install notes

`npm install --ignore-scripts` succeeds and was used only as an install smoke test. Generated `node_modules` and package-lock output are intentionally not part of the source ZIP.
