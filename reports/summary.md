# MCPace v0.6.0 source archive summary

Packaged: 2026-05-15 12:06:31 Europe/Copenhagen

## What this archive contains

- Rust source for the `mcpace` CLI/runtime.
- npm launcher and platform-package scaffolding under `packages/npm`.
- Project configs, schemas, presets, examples, tests, scripts, and documentation.
- Lifecycle, tool exposure, mixed-upstream, fail-safe, scale, and message-integrity hardening changes.

## What was intentionally excluded

- `.git`, `node_modules`, Rust `target`, `dist`, caches, temporary files, OS artifacts, generated proof reports, and stale prebuilt binaries.
- The npm platform package folders remain as source scaffolding, but native binaries must be rebuilt before publishing platform packages.

## Verification performed in this packaging environment

Passed:

- `npm run lint:npm`
- `npm run test:npm`
- `npm run audit:source -- --write reports/source-audit-final.json`
- `npm run verify:tool-message-integrity`
- `npm run verify:tool-exposure-safety`
- `npm run verify:tool-scale`
- `npm run verify:mixed-upstreams`
- `npm run verify:upstream-failsafe`

Not run in this sandbox:

- `cargo fmt`, `cargo check`, `cargo test`, and `cargo clippy`, because `cargo`/`rustc` were not available.
- Real-client traces for Claude Desktop, Cursor, and Windsurf.
- macOS/Windows/ARM native runtime execution.

## Recommended final verification on a Rust host

```bash
cargo fmt --all -- --check
cargo check --all-targets --locked
cargo test --all-targets --locked
cargo clippy --all-targets --locked -- -D warnings
npm run verify:rust-quality
npm run verify:local-prepublish
npm run verify:publish-decision
```

## Basic local run from source

```bash
cargo build --release
./target/release/mcpace version
./target/release/mcpace serve
```

For npm launcher smoke after building:

```bash
MCPACE_BINARY_PATH=./target/release/mcpace node packages/npm/cli/bin/mcpace.js version
```
