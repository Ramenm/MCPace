# Rust live proof

`npm run proof:rust-live` emits a non-blocking JSON report with schema `mcpace.rustLiveProof.v1`. It checks the pinned Rust toolchain, `rustc`, `cargo`, `rustfmt`, `clippy`, `Cargo.lock` synchronization, and the Cargo commands that prove the native binary when the tools are available.

`npm run proof:rust-live:enforce` is the fail-closed component gate and remains useful for focused diagnosis. The canonical release-host sequence does not run it separately: `npm run check:ci` delegates once to enforcing endgame, and endgame owns the single Rust proof/build run.

```bash
rustup toolchain install 1.95.0 --component rustfmt --component clippy
npm ci --ignore-scripts
npm run check:ci
```

The enforcing proof runs `cargo check --locked`, the single-threaded locked test suite, `cargo fmt --check`, locked Clippy with warnings denied, and `cargo build --release --locked --bins`. It never regenerates `Cargo.lock`. With `--write`, the report records before/after SHA-256 snapshots of every Rust build input and both proof generators. It blocks if any snapshot changes. The freshly built release artifact is independently hashed before and after final provenance collection.

The live MCP proof accepts only the canonical bound release artifact. It executes a private, hash-verified copy, hashes that private copy again after shutdown, revalidates the selected artifact and every proof input, and records all four artifact hashes. On Windows the proof process first joins a kill-on-close Job Object; on Unix it uses a dedicated process group. Cleanup tracks the harmless fixture PID, verifies that the leader and fixture are gone, and is a required passing proof step. Timestamps alone are not proof of provenance.

A lockfile change is dependency-maintenance work, not validation. Update only the intended package (for example with `cargo update -p <crate> --precise <version>`), review the complete `Cargo.lock` diff, and then rerun the enforcing proof.
