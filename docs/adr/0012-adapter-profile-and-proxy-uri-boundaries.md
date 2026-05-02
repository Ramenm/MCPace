# ADR 0012 — Adapter profile and proxy URI boundaries

## Status

Accepted for v0.5.5.

## Context

After the dashboard, upstream, MCP server, and source-registry splits, `src/adapter.rs` was still close to the large-module warning threshold and mixed separate concerns:

- adapter profile rendering from MCP `initialize` input;
- proxied upstream resource URI encoding and decoding;
- upstream projection orchestration;
- tool surface shaping and environment-derived adapter options.

Earlier extraction also left the adapter root depending on helper functions defined inside `src/adapter/discovery.rs`. Those helpers were intended for adapter-internal reuse, but their visibility was not explicit enough for a full Rust compile gate.

## Decision

Keep `src/adapter.rs` as the adapter root for types, options, tool-surface shaping, and projection orchestration. Extract focused behavior into child modules:

- `src/adapter/profile.rs` owns `adapter_profile` and client capability summaries derived from MCP `initialize` input.
- `src/adapter/proxy_uri.rs` owns MCPace proxied resource URI encoding/decoding and upstream error metadata helpers.
- `src/adapter/discovery.rs` keeps discovery/search/resource/prompt utilities and exposes the helper functions needed by the adapter root as `pub(super)`.

A Node source-quality contract verifies the split and the helper visibility markers.

## Consequences

### Positive

- The adapter root is smaller and below the focused split target.
- Profile rendering no longer grows the adapter root.
- URI encoding helpers are isolated from projection and tool-surface logic.
- The source tree has an explicit guard against private child-helper drift while the sandbox cannot complete Cargo check/test/build.

### Tradeoffs

- This is still source-level verification. It does not replace `cargo check`, `cargo test`, or real runtime tracing.
- Some projection orchestration remains in `src/adapter.rs`; moving it further should happen only after Rust compile/test is green.

## Follow-up

- Run full Cargo check/test/build on a host with dependency access.
- If adapter projection grows again, split projection into `src/adapter/projection.rs` after the Rust gate is available.
- Keep client capability behavior derived from MCP `initialize` and explicit config, not from hardcoded client maps.
