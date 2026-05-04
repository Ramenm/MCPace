# Merge report: mcpace v0.5.9 + offline quality pack

Generated: 2026-05-03

## Inputs

- `mcpace-v0.5.9-030526-124500(1).zip`
- `mcpace_offline_quality_20260503(1).zip`

## Merge strategy

The release/archive state from `mcpace-v0.5.9-030526-124500` was used as the base so the late release assets, local Rust compatibility crates, lockfile shape, and vendored npm binary were preserved. The offline quality pack was then layered in for security, quality, local-first publishing gates, GitHub readiness, documentation, and MCP HTTP session hardening.

No base-only or quality-only file paths were dropped. Common files were reconciled manually where the two archives had divergent edits.

## Preserved from v0.5.9 base

- Local Rust compatibility crates under `crates/compat/` and the matching `Cargo.toml`/`Cargo.lock` path dependency layout.
- Vendored Linux npm binary at `packages/npm/cli/vendor/linux-x64-gnu/mcpace`.
- Runtime resource environment override behavior in `src/resources.rs`/dashboard defaults.
- Release/publish npm pin `npm@11.13.0` and the trusted-publishing workflow shape.
- `scripts/ready-footprint-audit.mjs` and optimization reports.

## Added or reconciled from offline quality pack

- MCP HTTP session store and server-generated `Mcp-Session-Id` lifecycle.
- Host/Origin HTTP boundary hardening and explicit non-local bind guard.
- Dashboard overview metrics for HTTP session store state.
- GitHub/community readiness files and workflows, including CodeQL, dependency review, templates, SECURITY/SUPPORT/CODE_OF_CONDUCT/ROADMAP/CHANGELOG/CITATION.
- Local quality, bug sweep, defect gates, publish-decision, free-tier, secret scan, and supply-chain scripts.
- Additional Node contract tests and documentation for local-first/offline quality proof.

## Manual conflict fixes

- Kept base `npm@11.13.0` everywhere and updated quality tests/docs that still expected `11.12.1`.
- Kept base resource-env overrides while adding quality session/boundary code.
- Updated setup HTTP smoke probe to carry the server-issued `Mcp-Session-Id` from `initialize` into `tools/list`.
- Restored the existing dashboard smoke `#[test]` annotation that had been lost in a copied quality test file.
- Made npm archive/pack tests conditional on an actually present vendored binary and prevented tests from deleting a pre-existing vendor tree.
- Preserved both `--jobs` compatibility and `--batch-size` support in `scripts/run-node-test-files.mjs`.

## Verification

Passed:

- `cargo check --all-targets --locked`
- `cargo test --all-targets --locked`
- `node scripts/check-node-syntax.mjs --json --write reports/node-syntax-latest.json` — 99/99 files checked, 0 failures
- `node scripts/run-node-test-files.mjs --dir tests/node --ext .test.js ...` — 38/38 passed
- `node scripts/run-node-test-files.mjs --dir packages/npm/cli/test --ext .test.mjs ...` — 3/3 passed
- `node scripts/verify-npm-pack.mjs --json` — pass, `vendored-binary-bundle`
- `node scripts/verify-vendored-binary.mjs --json` — pass for `linux-x64-gnu`, version `0.5.9`
- `node scripts/defect-gates.mjs --json ...` — pass
- `node scripts/bug-sweep.mjs --json ...` — pass
- `node scripts/github-health-audit.mjs --json ...` — pass
- `node scripts/secret-scan.mjs --json ...` — pass
- `node scripts/free-tier-readiness.mjs --json ...` — ready

Warnings / environment-only blockers observed in this container:

- `verify-github-readiness` returned `ready-with-warnings`.
- `supply-chain-risk-audit` returned `pass-with-warnings` because optional tools such as `cargo-audit`, `cargo-deny`, and `gitleaks` are not installed.
- `tooling-readiness` returned `blocked` because this container has Node `v18.20.4` and npm `9.2.0`, while the project declares Node `>=22.0.0` and npm `>=10.0.0`.
- `publish-decision` returned `blocked` because release/native publication gates require the declared modern Node/npm toolchain and fresh full release evidence.

## Path coverage check

`reports/merge-path-coverage-20260503.json` records the path coverage check:

- base files: 434
- quality files: 538
- merged files: 553
- base-only paths missing from merged: 0
- quality-only paths missing from merged: 0
