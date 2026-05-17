# Rust host mirror proof — 2026-05-17

Status: `pass-with-release-build-blocked`

## What changed

- OpenAI Artifactory Debian mirrors were configured for apt in this environment.
- Rust tooling was installed from the mirrored Debian packages.
- Cargo was able to work offline/locked after regenerating `Cargo.lock` so the package version matches `0.6.5`.

## Toolchain

| Tool | Version |
|---|---|
| rustc | `rustc 1.85.0 (4d91de4e4 2025-02-17)` |
| cargo | `cargo 1.85.0 (d73d2caf9 2024-12-31)` |
| rustfmt | `rustfmt 1.8.0` |
| clippy | `clippy 0.1.85` |

Project pin: `rust-toolchain.toml requests 1.95.0`

Pin status: not satisfied by rustup because no static.rust-lang.org/rustup mirror endpoint was available in this environment; Debian mirror toolchain was used instead

## Checks

| Check | Status | Detail |
|---|---|---|
| `cargo-generate-lockfile-offline` | `pass` | Cargo.lock regenerated offline so mcpace package version matches 0.6.5. |
| `cargo-fmt-check` | `pass` | cargo fmt --all -- --check |
| `cargo-check-all-targets` | `pass` | cargo check --locked --all-targets |
| `cargo-clippy-all-targets` | `pass` | cargo clippy --locked --all-targets -- -D warnings |
| `cargo-test-lib` | `pass` | cargo test --locked --lib -j1; 100 passed |
| `cargo-test-integration-client_surface` | `pass` | 31 passed |
| `cargo-test-integration-config_and_server` | `pass` | 11 passed |
| `cargo-test-integration-help_and_root` | `pass` | 8 passed |
| `cargo-test-integration-hub_leases` | `pass` | 6 passed |
| `cargo-test-integration-hub_runtime` | `pass` | 7 passed |
| `cargo-test-integration-lab_surface` | `pass` | 3 passed |
| `cargo-test-integration-mcp_server` | `pass` | 8 passed |
| `cargo-test-integration-service` | `pass` | 2 passed |
| `cargo-test-integration-setup` | `pass` | 3 passed |
| `cargo-test-integration-stdio_shim` | `pass` | 2 passed |
| `cargo-test-doc` | `pass` | 0 doc tests |
| `cargo-build-debug-bin` | `pass` | cargo build --locked --bin mcpace -j1; version output `0.6.5` |
| `cargo-build-release-bin` | `blocked` | Killed by the constrained tool runtime during optimized release/LTO link; no optimized release binary proof produced. |

## Remaining blocker

The optimized release build was not proven in this constrained environment because the release/LTO link was killed by the tool timeout. The debug binary build and Rust source quality gates were proven.
