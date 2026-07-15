# Endgame readiness

`npm run check:endgame` combines the final static and live-proof gates into one machine-readable report with schema `mcpace.endgameReadiness.v1`.

The non-enforcing mode is allowed to report `status: "blocked"` and exit successfully in source-review or sandbox environments. This keeps the exact blockers visible without pretending the native product has been proven.

`npm run check:endgame:enforce` is the release-host gate. It fails closed unless all of these are true:

- MCP stdio and Streamable HTTP source contracts pass;
- Rust boundary contract passes, including typed error seams and the raw HTTP/TCP allowlist;
- supply-chain evidence has no blockers;
- release readiness has no blockers;
- pinned Rust tools are installed;
- `Cargo.lock` is synchronized with `Cargo.toml`;
- locked Cargo check, tests, formatting, clippy, and release build pass;
- the enforcing run writes a Rust report that binds the unchanged full build-input fingerprint to the release binary SHA-256;
- source archive hygiene still passes.

This gate is deliberately stricter than the fast local `npm run check`: a local contributor can work without Rust installed, but a release host cannot publish while Rust proof or lockfile proof is missing. `check:ci` then runs the live dashboard/MCP path against that exact release binary and writes the source-bound live report.
