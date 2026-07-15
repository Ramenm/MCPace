# Release readiness gate

`npm run check:release-ready` is a non-blocking source-tree report. It is allowed to return JSON with `status: "blocked"` and exit successfully so local contributors can see exactly what is missing without breaking fast CI or source review.

`npm run check:release-ready:enforce` is the fail-closed gate for a release host. It exits non-zero if any release blocker remains.

The gate checks:

- the pinned Rust toolchain from `rust-toolchain.toml`;
- `rustc`, `cargo`, `rustfmt`, and `clippy` availability;
- `Cargo.toml` / `Cargo.lock` synchronization for standard crates that were migrated away from compatibility shims;
- CI script wiring for `check:ci`, `check:mcp-transport`, `check:rust-boundaries`, supply-chain evidence, Rust live proof, endgame readiness, and release-readiness itself;
- npm trusted-publishing/provenance workflow shape;
- GitHub release artifact attestation workflow shape.

A Rust-enabled release machine should run this sequence before claiming a release is fully proven:

```bash
rustup toolchain install 1.95.0 --component rustfmt --component clippy
npm ci --ignore-scripts
npm audit --audit-level=moderate --json
npm audit signatures
cargo build --release --locked --bins
npm run check:ci
```

`check:ci` runs the fail-closed release-readiness gate and the endgame gate. Endgame owns the single live Rust `check`/`test`/`fmt`/`clippy` proof, and the CI entrypoint also performs the release-artifact dry run. Do not regenerate `Cargo.lock` during validation. If an intentional dependency update is required, update only the named package, review the lockfile diff, then restart the full locked sequence.

The checker produces a machine-readable report with schema `mcpace.releaseReadiness.v1`.
