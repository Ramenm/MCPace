# Source map pass — 2026-05-16

## Scope

Second autonomous pass over the patched `mcpace` source archive after the maintainer supplied the grounded-work operating rules.

## Checked project sources

- Manifests and package versioning: `Cargo.toml`, `Cargo.lock`, `package.json`, `packages/npm/*/package.json`, `mcpace.config.json`.
- Archive contract: `release-manifest.json`, `scripts/archive-release.mjs`, `tests/node/archive-contract.test.js`, `reports/summary.md`.
- HTTP/MCP safety: `src/dashboard.rs`, `src/dashboard/http_boundary.rs`, `src/dashboard/http_session.rs`, `src/dashboard/mcp_http.rs`, `docs/mcp-http-api-spec.md`, `tests/node/security-contract.test.js`.
- Eval governance: `docs/eval-plan.md`, `eval/README.md`, `eval/scenario-matrix.json`, `eval/scoring-rubric.json`, `eval/dataset-plan.json`, `eval/runtime-capabilities.json`, `eval/fixtures/**`, `tests/node/eval-contract.test.js`, `tests/node/fixtures-contract.test.js`.
- Product truth and proof framing: `docs/product-truth.json`, `reports/verification-latest.json`, `tests/node/product-truth-contract.test.js`, `scripts/proof-report.mjs`.

## External references used for this pass

- MCP Streamable HTTP/security docs: session, local HTTP, origin, and auth posture.
- OpenAI eval guidance: evals as repeatable checks around prompt/model/tool behavior.
- OWASP CI/CD Top 10: artifact integrity and pipeline evidence risks.
- Rust `getrandom` docs: operating-system randomness as the intended entropy source.

## Confirmed facts

- The repo already has eval governance files and contract tests for scenario maps, rubrics, held-out cases, and runtime fixtures.
- The previous archive manifest included the entire `reports` directory, while `reports/summary.md` claimed generated reports were intentionally excluded.
- Multiple historical reports with older versions exist under `reports/`; they are useful as local history but unsafe to bundle wholesale as current release evidence.
- Node/npm source checks can run in this sandbox. Rust host checks still require `cargo`/`rustc` on a real Rust host.

## Actions taken in this pass

- Bumped the working snapshot from `0.6.0` to `0.6.2` in current manifests and eval/product-truth metadata.
- Added `docs/developer-operating-mode.md` from the maintainer's raw working rules.
- Changed `release-manifest.json` so release archives include selected useful report artifacts instead of the entire historical `reports` directory.
- Added archive-contract assertions preventing old generated report bundles from silently re-entering the ZIP.
- Added this source map plus technical-debt and eval-readiness reports.

## Still unconfirmed

- Rust format/check/test/clippy/release build, because the current sandbox has no Rust toolchain.
- Native platform package proof for macOS, Windows, and ARM targets.
- Public publishing/provenance proof.
