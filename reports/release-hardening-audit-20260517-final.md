# Final hardening audit — 2026-05-17

## Scope

This pass reviewed MCPace as a source hardening snapshot, not as a final native binary publication. The goal was to reduce embarrassing release risk around random MCP server discovery, policy classification, environment leakage, and evidence claims.

## Fixes made in this pass

- Upgraded live random probe evidence to `mcpace.liveRandomMcpProbe.v4`.
- Fixed fixture replay safety semantics: replay reports now say `executesThirdPartyPackages=false` even when the underlying saved evidence came from a previous live package-manager run.
- Added package-manager install hardening: npm/uv installs now use a whitelisted environment, isolated HOME/cache/tmp directories, disabled npm lifecycle scripts, timeouts, and process-tree termination.
- Replaced install subprocesses that inherited `process.env` with `cleanPackageManagerEnv(...)` and `runCommandWithTimeout(...)`.
- Added extra live canary classes: official Playwright, Google Maps, Azure, and EVM/blockchain.
- Added classifier policies for `cloud-admin-credential-review` and `blockchain-wallet-review`.
- Expanded risk signals for mutable/destructive tool annotations, open-world annotations, prompt-injection-looking descriptions, cloud-admin, and blockchain/wallet behavior.
- Expanded Registry Lab to `mcpace.registryLab.v2` and added metadata-only examples for cloud admin, blockchain/wallet, and prompt-injection-looking registry descriptions.
- Converted the live random probe contract test back to CommonJS to avoid Node module-type warnings in the project’s mixed CommonJS/ESM test tree.

## Real package-manager probes run in this pass

### npm stable, sanitized install env

`reports/live-random-mcp-probe-npm-stable.json`

- `official-filesystem`: OK, 14 tools, `project-filesystem-single-writer`, handled `roots/list`.
- `official-memory`: OK, 9 tools, `state-profile-single-session`.
- `official-sequential-thinking`: OK, 1 tool, `state-profile-single-session`.

### PyPI/uv, sanitized install env

`reports/live-random-mcp-probe-pypi-latest.json`

- `python-time`: OK, 2 tools, `local-utility-multi-reader`.
- `python-git`: OK, 12 tools, `project-repo-single-writer`.
- `python-fetch`: OK, 1 tool, `network-fetch-review`.
- `python-sqlite`: OK, 6 tools, `database-path-single-writer`.

### Additional npm canaries

- `reports/live-random-mcp-probe-playwright-canary.json`: OK, 23 tools, `shared-exclusive-host-lock`.
- `reports/live-random-mcp-probe-google-maps-canary.json`: expected startup-error without API key, `credential-scoped-review`.
- `reports/live-random-mcp-probe-azure-canary.json`: expected startup-error/missing command without cloud scope, `cloud-admin-credential-review`.
- `reports/live-random-mcp-probe-evm-canary.json`: OK, 25 tools, `blockchain-wallet-review`.

## Checks run

- `npm run verify:hardening`: pass.
- `npm run test:repo:smoke`: pass.
- `npm run test:npm`: pass.
- `npm run verify:vendored-binary`: pass.
- `npm run verify:runtime-trace`: pass.
- `npm run verify:secrets`: pass.
- `node --test tests/node/live-random-mcp-probe-contract.test.js tests/node/registry-lab-contract.test.js`: pass.
- `node --check scripts/live-random-mcp-probe.mjs`: pass.
- `node --check scripts/registry-lab.mjs`: pass.

## Critical critique

MCPace is much stronger than the first archive, but the honest public claim must stay conservative.

What is credible now:

- MCPace can discovery-probe pinned npm/PyPI MCP servers without calling tools.
- Unknown/random servers remain review-gated.
- State/profile/project/browser/cloud/blockchain/database/shell classes are classified conservatively.
- The probe no longer inherits arbitrary user environment variables into package-manager installs or runtime processes.
- The probe handles server-side `roots/list` and `ping` during discovery instead of hanging.

What is not credible yet:

- Safe execution of arbitrary tools from arbitrary MCP servers.
- Production-safe Docker MCP server support.
- Production-safe remote Streamable HTTP upstream session pooling.
- Rust source changes being represented by the vendored binary.
- Full release readiness without Cargo/Rust CI proof.

## Remaining release blockers

1. Rebuild the Rust binary from this source and run `cargo fmt`, `cargo check`, `cargo test`, and `cargo clippy` on a real Rust host.
2. Add Docker/chroot/firejail or equivalent sandbox lane before any destructive random-server tool-call tests.
3. Add concurrency torture for real stateful servers: same browser profile, same git repo, same SQLite DB, same memory profile, cancel/timeout/crash/stale-response cases.
4. Implement/polish remote HTTP upstream session pooling before claiming stateful Streamable HTTP upstream support.
5. Keep the UI wording conservative: “review required” and “disabled until confirmed” for unknown/cloud/wallet/command/openapi/cluster classes.

## Release wording recommendation

Use:

> MCPace can discovery-probe pinned npm/PyPI MCP servers, classify risky server classes conservatively, and keep unknown/problematic servers review-gated. It does not claim safe destructive execution of arbitrary MCP servers without a separate sandbox and policy review.

Do not use:

> MCPace safely supports every random MCP server.
