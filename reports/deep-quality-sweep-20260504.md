# Deep quality sweep — 2026-05-04

Generated: `2026-05-04T09:54:11Z`

## What changed
- **Rust source quality**: Formatted Rust source and expanded verify-rust-quality to fmt -> cargo check -> clippy -> full suite-isolated Rust tests -> release build with a longer cold-build timeout.
- **Product proof hygiene**: Added actionable nextMoves for stale proof reports in product-practice harness.
- **npm package proof**: Added --write support and schema/generatedAt/reportPath to verify-npm-pack and changed npm script to persist reports/verify-npm-pack-latest.json.
- **Tests**: Added/updated contract tests for full Rust-quality policy, product-practice ready vs risk states, and npm-pack persisted proof reports.
- **Docs**: Updated source quality, test strategy, verification matrix, and release engineering docs to match the stronger gates.

## Confirmed checks
| Check | Result |
|---|---|
| Rust quality | pass — fmt, check, clippy, rust-tests, release-build; profile `full` |
| Node repo tests | pass — 38/38 files, 5 batches |
| npm CLI tests | pass — 3/3 files |
| npm pack proof | pass — `vendored-binary-bundle`, vendored details `[{'path': 'vendor/linux-x64-gnu/mcpace', 'mode': 493, 'size': 1786864}]`, non-exec `[]` |
| Vendored binary proof | pass — target `linux-x64-gnu`, version `0.5.9` |
| Runtime trace | pass |
| Boot harness | generated; npm pack `pass`, Rust available `True`, Node supported `False` |
| Install readiness | ready-with-warnings — warnings `['current Node v18.20.4 is below project policy >=22.0.0', 'current npm 9.2.0 is below project policy >=10.0.0']` |
| Product practice | ready-for-release-candidate-review — canClaim `{'sourceTreeHealthy': True, 'sourceThinLauncherInstall': True, 'runtimeBeta': True, 'publishedBinaryInstall': True, 'universalRemoteMcpBroker': False}` |
| Local source suite | pass-with-warnings |
| Local prepublish | blocked |
| Publish decision | source-ready-publish-blocked — next `['Run npm run verify:local-prepublish on the release host.', 'Review supply-chain warnings before a polished launch.']` |
| Defect gates / bug sweep | pass / pass |
| GitHub / secrets / free-tier | pass / pass / ready |
| Supply chain / tooling | pass-with-warnings / blocked |

## Not counted
- `npm run test:node:coverage` was not counted as pass on this host: Node `v18.20.4` rejects `--test-force-exit`; without the flag, coverage output reached `end of coverage report` but the process did not exit before the timeout.

## Remaining risks
- Official tooling gate remains blocked in this container because Node is v18.20.4 and npm is 9.2.0 while project policy requires Node >=22 and npm >=10.
- local-prepublish and publish-decision remain blocked for release publication because tooling/public-release proof is not complete on this host.
- supply-chain audit is pass-with-warnings until independent tools such as cargo-audit, cargo-deny, gitleaks, osv-scanner, and trivy are installed.
- Node coverage command was not counted as pass on this Node 18 host; rerun on Node >=22/npm >=10 release host.

## Best next move
Move the same archive to a Node >=22/npm >=10 release host, install cargo-audit/cargo-deny/gitleaks/osv-scanner, then rerun verify:tooling, test:node:coverage, verify:local-prepublish, and verify:publish-decision.
