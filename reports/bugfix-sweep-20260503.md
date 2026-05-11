# MCPace bugfix sweep — 2026-05-03

Generated: `2026-05-03T19:12:52Z`

## Bugs fixed

### vendored-binary-rust-toolchain-false-negative
- Severity: `high`
- Symptom: verify:vendored-binary failed on a valid staged binary when Cargo/Rust was not installed on the host.
- Root cause: Binary-install verification required doctor.rustSourceReady, conflating consumer/runtime binary verification with source-build verification.
- Fix: verify-vendored-binary now requires config/npm readiness for the binary smoke, records rustSourceReady as informational, and adds schema/generatedAt/project metadata.
- Regression proof: tests/node/verify-vendored-binary.test.js: runtime host without Rust toolchain, npm run verify:vendored-binary

### vendored-binary-report-freshness-gap
- Severity: `medium`
- Symptom: publish-decision/product-practice could not rely on a fresh vendored-binary report after the verifier ran.
- Root cause: verify-vendored-binary printed JSON but did not persist reports/vendored-binary-latest.json; release workflows also did not persist per-target proof reports.
- Fix: Added --write support; package script writes reports/vendored-binary-latest.json; release workflows write reports/vendored-binary-<target>.json.
- Regression proof: tests/node/verify-vendored-binary.test.js: write report test, release workflow contract tests

### product-practice-published-binary-overclaim
- Severity: `high`
- Symptom: product-practice could mark published-binary-install as pass based on install-readiness/binary presence rather than direct binary verification proof.
- Root cause: The claim gate used boot/install readiness instead of a fresh, target-matching vendored-binary proof report.
- Fix: product-practice now requires reports/vendored-binary-latest.json or reports/vendored-binary-<target>.json to be fresh, status pass, and match the current host target.
- Regression proof: tests/node/product-practice-contract.test.js, npm run verify:product-practice

### product-practice-runtime-beta-overclaim
- Severity: `high`
- Symptom: product-practice could expose canClaim.runtimeBeta=true while rust-build was blocked/failing.
- Root cause: runtimeBeta was tied only to runtime trace usability, despite the harness status requiring Rust proof before runtime claims.
- Fix: runtimeBeta now requires both fresh passing Rust quality proof and fresh passing runtime trace proof.
- Regression proof: tests/node/product-practice-contract.test.js: runtimeBeta equals rust-build pass AND runtime-trace pass

## Test and gate summary

| check | status | evidence |
|---|---:|---|
| node syntax | `pass` | 99/99 checked, failures=0 |
| npm CLI tests | `pass` | 3/3 passed |
| npm pack | `ready-with-warnings` | vendored-binary-bundle |
| vendored binary | `pass` | linux-x64-gnu version=0.5.9 runtimeOps=True |
| runtime trace | `pass` | spawned-local-serve linux-x64-gnu packages/npm/cli/vendor/linux-x64-gnu/mcpace |
| rust quality | `fail` | cargoAvailable=False firstLane=fmt error=spawnSync cargo ENOENT |
| product practice | `prove-rust-before-runtime-claims` | runtimeBeta=False publishedBinaryInstall=True |
| publish decision | `blocked` | source=False native=False blockers={'totalGates': 10, 'passed': 6, 'warnings': 1, 'sourceBlockers': 1, 'releaseBlockers': 3} |
| bug sweep | `pass-with-warnings` | warnings=1 blocked=0 |
| defect gates | `pass` | blocked=0 warnings=0 |
| secret scan | `pass` | critical=0 warnings=0 |
| supply-chain risk | `pass-with-warnings` | blockers=0 warnings=5 |
| free-tier readiness | `ready` | blockers=0 warnings=0 |
| github health | `pass` | blocked=0 total=30 |
| github readiness | `ready-with-warnings` | warnings=1 required=24/24 |
| tooling readiness | `blocked` | blocked=4 warnings=4 |

Node repo contract groups:

- `reports/node-tests-group1.json`: `pass`, 8/8 passed
- `reports/node-tests-group2.json`: `pass`, 8/8 passed
- `reports/node-tests-group3.json`: `pass`, 8/8 passed
- `reports/node-tests-group4.json`: `pass`, 8/8 passed
- `reports/node-tests-group5.json`: `pass`, 6/6 passed

## Limitations / not claimed

- cargo, rustc, rustfmt, and clippy are not installed in this container; Rust source quality commands fail with command not found / spawnSync cargo ENOENT.
- Rust source compile/test/fmt/clippy results must be rerun on a Rust host before native publication.
- Full Node coverage command was attempted but the container timed it out; grouped Node contract tests completed 38/38 pass and coverage-contract itself passed.
- Supply-chain audit is pass-with-warnings because optional external scanners are not installed in this container.

## Changed paths

- `.github/workflows/release-dry-run.yml`
- `.github/workflows/release.yml`
- `docs/offline-quality-and-publish-gates.md`
- `docs/product-practice.md`
- `package.json`
- `reports/bug-sweep-latest.json`
- `reports/bug-sweep-latest.md`
- `reports/defect-gates-latest.json`
- `reports/defect-gates-latest.md`
- `reports/free-tier-readiness-latest.json`
- `reports/github-health-latest.json`
- `reports/github-readiness-latest.json`
- `reports/install-readiness-latest.json`
- `reports/local-quality-source-latest.json`
- `reports/local-quality-source-latest.md`
- `reports/node-syntax-latest.json`
- `reports/node-tests-smoke-latest.json`
- `reports/npm-cli-tests-latest.json`
- `reports/product-practice-latest.json`
- `reports/product-practice-latest.md`
- `reports/publish-decision-latest.json`
- `reports/publish-decision-latest.md`
- `reports/runtime-trace-latest.json`
- `reports/runtime-trace-latest.md`
- `reports/rust-quality-latest.json`
- `reports/secret-scan-latest.json`
- `reports/secret-scan-latest.md`
- `reports/source-audit-latest.json`
- `reports/supply-chain-risk-latest.json`
- `reports/toolbox-doctor-latest.json`
- `reports/toolbox-doctor-latest.md`
- `reports/tooling-readiness-latest.json`
- `reports/tooling-readiness-latest.md`
- `reports/vendored-binary-latest.json`
- `scripts/product-practice-harness.mjs`
- `scripts/verify-vendored-binary.mjs`
- `tests/node/product-practice-contract.test.js`
- `tests/node/verify-vendored-binary.test.js`
- `reports/node-tests-group1.json`
- `reports/node-tests-group2.json`
- `reports/node-tests-group3.json`
- `reports/node-tests-group4.json`
- `reports/node-tests-group5.json`
