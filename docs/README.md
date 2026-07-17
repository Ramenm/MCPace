# MCPace 0.8.2 — build and verification guide

## Prerequisites

- Node.js 22 and npm (use the repository lockfile).
- Rust toolchain selected by `rust-toolchain.toml` when present; otherwise use the version documented in the root README/CI.
- A supported desktop OS. The dashboard binds to loopback and must not be exposed through an unauthenticated network proxy.

## Install

```bash
npm ci --ignore-scripts
```

Rust dependencies are resolved by Cargo during the first Rust check/build.

## Run

Use the canonical commands in the root `README.md` and `package.json`. Common entry points are:

```bash
npm test
npm run check
cargo run --bin mcpace -- help
```

Before using HTTP mutation endpoints, configure the project’s HTTP authentication token and review the security notes in `SECURITY.md` and `docs/research/security-hardening-sources.md`. The command replacement table is in `docs/cli-migration.md`. The intentionally small CLI, hidden compatibility entrypoints, upstream project comparison, and no-reboot startup proof boundary are documented in `docs/research/cli-and-autostart-patterns.md`.

## Full local verification

```bash
npm test
npm run check
cargo fmt --all -- --check
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
npm run test:rust
```

Some assurance/readiness commands intentionally report a blocked state until a live Rust binary proof is regenerated from the exact current source snapshot. A blocked proof must not be relabelled as passing. This source bundle is not production-release approval; publish only after every enforced release gate passes on the signed build hosts.

## Package contents

This source package intentionally excludes `.git`, dependencies, caches, local agent/browser/IDE state, temporary files, and heavy build outputs. `reports/summary.md` contains the exact verification status captured during packaging.
