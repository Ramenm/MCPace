# Rust Tooling Policy

## Immediate baseline

- keep the core implementation in Rust
- keep the npm layer thin and auditable
- add crates only when they reduce real complexity

## Selected next-layer tools

- `clap` for grouped CLI parsing
- `rmcp` for protocol/runtime layer
- `tokio` for async runtime
- `schemars` + `jsonschema` for config/schema alignment
- `anyhow` + `thiserror` for error structure
- `tracing` stack for structured logs
- `cargo-nextest`, `cargo-audit`, `cargo-deny` for later governance once build proof exists
