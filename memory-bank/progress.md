# Progress

- Version: `0.6.0`.
- Status: source/proof/native-BYO-MCP usability improved; product-practice and runtime-trace gates now prevent overclaiming. Runtime/beta readiness still blocked by Rust host proof and real-client trace.

## Completed in latest pass

- Replaced hardcoded `lint:npm` file list with `scripts/check-node-syntax.mjs` auto-discovery.
- Added `verify:product-practice` and `verify:runtime-trace` npm scripts.
- Added `scripts/product-practice-harness.mjs` and `scripts/runtime-trace-harness.mjs`.
- Added tiny stdio MCP fixture: `tests/fixtures/tiny-mcp-stdio-server.mjs`.
- Added docs: `START-HERE.md`, `docs/product-practice.md`.
- Added release-manifest coverage for `START-HERE.md`.
- Added/updated Node contract tests for source lint discovery, product practice, boot harness, and runtime trace harness.
- Regenerated reports for inventory, boot, install readiness, product practice, runtime trace, source audit, and Rust quality.

## Verified

- `cargo fmt --all -- --check` — PASS.
- `npm run lint:npm` — PASS.
- Repo Node test files were covered in split runs; a single continuous repo runner still times out in this sandbox.
- `npm run test:npm` — PASS.
- `node scripts/audit-source.mjs --json --write reports/source-audit-latest.json` — PASS.
- `node scripts/verify-npm-pack.mjs --json` — PASS; package mode is `thin-launcher`.
- `node scripts/boot-harness.mjs --json --write reports/boot-harness-latest.json --markdown reports/boot-harness-latest.md` — PASS with `partial` readiness because this environment is Node 18/npm 9 and no native binary is staged.
- `node scripts/install-readiness-harness.mjs --json --write reports/install-readiness-latest.json` — PASS with `ready-with-warnings`.
- `node scripts/product-practice-harness.mjs --json --write reports/product-practice-latest.json --markdown reports/product-practice-latest.md` — PASS; runtime and published-install claims remain blocked.
- `node scripts/runtime-trace-harness.mjs --json --write reports/runtime-trace-latest.json --markdown reports/runtime-trace-latest.md` — PASS as a harness; runtime trace remains blocked by missing compiled/staged binary.

## Blocked / next proof

- Run Cargo check/test/build on a host with dependency access.
- Stage at least one platform native binary before calling npm published install ready.
- Record a real-client runtime trace through `/mcp` and the tiny stdio upstream fixture.
- Implement durable HTTP session store and remote Streamable HTTP upstream connector after runtime proof.
