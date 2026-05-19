# Changelog

All notable user-facing changes should be recorded here. Keep this file human-readable: focus on behavior, install/release impact, compatibility, security, and migration notes rather than every internal refactor.

## Unreleased

### Added

- Public GitHub launch kit: support policy, code of conduct, public issue templates, release-notes categorization, and a local GitHub health audit.
- Stronger product-proof hygiene: runtime trace reports carry host target metadata, and product-practice checks reject stale or host-mismatched proof before allowing runtime beta claims.
- Target-aware runtime trace binary discovery for `packages/npm/cli/vendor/<target>/mcpace` alongside local release/debug binaries.
- In-process Streamable HTTP session lifecycle for `/mcp`: create/reuse/touch, missing/unknown/expired/protocol-mismatch rejection, diagnostics, and `DELETE /mcp` close behavior.

### Changed

- README and repo docs now point contributors toward launch readiness, support boundaries, and release gates instead of broad unproven product claims.

### Still blocked before beta

- Fresh real-host proof for the in-process HTTP session lifecycle, plus any cross-process/relay-grade persistence needed after beta.
- HTTP/Streamable HTTP upstream forwarding.
- Real-client runtime traces through at least one tier-1 local client.
- Published native binary packages with checksums, attestations, and npm Trusted Publishing proof.

## 0.6.2

- Added `npm run verify:performance`, a source-level performance smoke harness that records HTTP benchmark wiring plus bounded tool-scale, mixed-upstream, and upstream-failsafe simulations.
- Added `docs/performance-verification.md` and bundled fresh `reports/performance-smoke-latest.*` artifacts in the source archive.
- Kept release performance claims gated on real Rust host p50/p95/p99 and memory baselines.

## 0.6.1

- Tightened source archive hygiene: generated historical reports are no longer included wholesale in release archives.
- Added a maintainer operating-mode document for grounded task intake, tech-debt prioritization, eval governance, and cautious high-risk answers.
- Added a second-pass technical-debt report and kept eval/version metadata aligned with the current source snapshot.

## 0.6.0

- Source package refresh: message-integrity hardening, tool exposure guards, lifecycle/scale/failsafe checks, and clean source archive packaging.
- Prebuilt binaries are intentionally omitted from this source ZIP; rebuild with the Rust toolchain before publishing platform packages.

## 0.5.9

### Current status

- Rust-first local MCP hub/control-plane source tree.
- Local `/mcp` endpoint and stdio MCP compatibility lane.
- BYO upstream registry, preset-based onboarding, client install/export surfaces, release/proof harnesses, and platform package scaffolding.
- Runtime and release claims remain proof-gated by `docs/product-truth-and-beta-gate.md`.
