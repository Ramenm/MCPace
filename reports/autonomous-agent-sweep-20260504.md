# Autonomous agent sweep — 2026-05-04

## Scope
Started from `mcpace-v0.5.9-030526-191500-bugfix.zip` and treated it as the current source of truth.

## Implemented fixes
- `scripts/product-practice-harness.mjs` — Published-binary gate now keeps the vendored-binary report path in blocked evidence, including stale-proof cases.
- `scripts/verify-npm-pack.mjs` — npm pack dry-run proof now records package entry mode/size details and fails when packed vendored binaries are non-executable on POSIX.
- `tests/node/verify-npm-pack.test.js` — Added regression coverage for executable-mode metadata and non-executable vendored binary rejection.
- `scripts/archive-release.mjs` — Archive staging explicitly preserves POSIX file modes after copy.
- `tests/node/archive-contract.test.js` — Release archive contract now checks that the linux vendored binary is archived with executable mode when present.
- `packages/npm/cli/vendor/linux-x64-gnu/mcpace` — Restored executable mode in the working tree after Python ZipFile extraction dropped it; final archive stores it as 0755.

## Checks
| check | exit |
|---|---:|
| lint_npm | 0 |
| audit_source | 0 |
| test_repo | 0 |
| test_npm | 0 |
| verify_npm_pack | 0 |
| verify_vendored_binary | 0 |
| verify_runtime_trace | 0 |
| verify_product_practice | 0 |
| verify_defect_gates | 0 |
| verify_bug_sweep | 0 |
| verify_github_health | 0 |
| verify_secrets | 0 |
| verify_supply_chain | 0 |
| verify_free_tier | 0 |
| verify_publish_decision | 0 |
| verify_tooling | 0 |
| verify_rust_quality | 1 |
| test_node_coverage | 0 |
| verify_local_smoke | 0 |
| verify_local_source | 0 |

## Blockers kept honest
- **rust-toolchain** — npm run verify:rust-quality exits 1; reports/rust-quality-latest.json records spawnSync cargo ENOENT.
- **publish-decision** — reports/publish-decision-latest.json status is blocked because Rust/local-prepublish proof is not available.

## Next best move
Run the Rust host lane on a machine with Cargo/Rust installed: `npm run verify:rust-quality`, then rerun `npm run verify:local:source` and `npm run verify:publish-decision`.
