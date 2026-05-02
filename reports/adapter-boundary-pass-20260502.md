# Adapter boundary pass — v0.5.5 / 2026-05-02

## Goal

Continue the reversible module split without changing the runtime product contract. The focus was the adapter layer because it is the bridge between MCP client-facing tool surfaces and upstream MCP discovery/call behavior.

## What changed

- `src/adapter/profile.rs` now owns `adapter_profile` and client capability summarization derived from `initialize` input.
- `src/adapter/proxy_uri.rs` now owns MCPace proxied resource URI encoding/decoding and helper metadata for upstream method errors.
- `src/adapter.rs` stays as the adapter root for public types, tool-surface shaping, projection orchestration, and environment-derived options.
- `src/adapter/discovery.rs` now marks adapter-root helper functions as `pub(super)` where the root legitimately uses extracted discovery utilities.

## Why this matters

The previous split reduced the large module count, but `adapter.rs` was still close to the audit threshold and mixed profile rendering, proxy URI encoding, and projection orchestration in one file. This pass separates those concerns and also fixes a possible Rust visibility drift where the adapter root depended on child-module helpers that were still private to `discovery.rs`.

## Verification

- `cargo fmt --all -- --check` — PASS.
- `node --test tests/node/source-quality-contract.test.js` — PASS, 9/9 subtests.
- `node scripts/audit-source.mjs --json --write reports/source-audit-latest.json` — PASS, `critical: []`, `warnings: []`, `largeModules: 0`.
- `npm test` — PASS after the adapter boundary contract was added.

## Still not verified

- `cargo check --all-targets --locked`, `cargo test --all-targets --locked`, and release build remain blocked in this sandbox by dependency resolution for crates.io.
- Real external MCP client runtime trace remains unverified.
- Durable HTTP session store and remote Streamable HTTP upstream forwarding remain future P1 work.
