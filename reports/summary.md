# MCPace v0.6.5 source archive summary

Packaged: 2026-05-16 12:45:00 Europe/Copenhagen

## What this archive contains

- Rust source for the `mcpace` CLI/runtime.
- npm launcher and platform-package scaffolding under `packages/npm`.
- Project configs, schemas, presets, examples, tests, scripts, and documentation.
- Eval governance assets under `eval/` plus `docs/eval-plan.md`.
- Maintainer operating rules in `docs/developer-operating-mode.md`.
- Performance verification guidance in `docs/performance-verification.md`.
- MCP server install scenario guidance in `docs/mcp-server-install-scenarios.md`.
- Lifecycle/blast-radius guidance in `docs/mcp-lifecycle-blast-radius.md`.
- Dashboard tab/refresh chaos guidance in `docs/dashboard-chaos-verification.md`.
- Selected current report artifacts only: this summary, product/proof snapshot, Rust command coverage, toolchain support, tombstones, source/eval/technical-debt reports, fresh performance smoke reports, dashboard chaos smoke reports, MCP install scenario smoke reports, lifecycle/blast-radius smoke reports, real Playwright dashboard E2E reports, external-tool/internet scenario reports, install-readiness, and vendored-binary proof reports.

## What was intentionally excluded

- `.git`, `node_modules`, Rust `target`, `dist`, caches, temporary files, OS artifacts, and nested compressed artifacts.
- The historical/generated report directory is not bundled wholesale. Old `*-latest.json` files can be useful local history, but they must not travel in the release ZIP as current proof.
- The npm platform package folders remain as source scaffolding. Native binaries must be rebuilt and freshly verified before publishing platform packages.

## Third-pass changes

- Bumped this changed source snapshot to `0.6.2`.
- Added `scripts/performance-smoke.mjs` and `npm run verify:performance`.
- Added `docs/performance-verification.md` with a two-layer performance proof model: source-level smoke regression plus host-specific Rust binary proof.
- Added `reports/performance-smoke-latest.json` and `reports/performance-smoke-latest.md` to the selected archive report allowlist.
- Restored missing evidence artifacts: `reports/rust-command-coverage.md`, `reports/toolchain-support.json`, and `reports/client-surface-spec-research-2026-04-17.md`.
- Added regression coverage for the performance smoke harness and archive inclusion of performance proof artifacts.
- Kept fixed latency thresholds opt-in until real Ubuntu/macOS/Windows baselines exist.


## Fourth-pass changes

- Added executable MCP install scenario smoke coverage through `scripts/mcp-install-scenario-smoke.mjs` and `npm run verify:mcp-install-scenarios`.
- Added `docs/mcp-server-install-scenarios.md` to clarify that MCPace install/register writes settings fragments and does not download packages, start servers, call remote endpoints, or invoke tools during registration.
- Added `reports/mcp-install-scenario-smoke-latest.json`, `reports/mcp-install-scenario-smoke-latest.md`, `reports/mcp-install-scenario-matrix-20260516.md`, `reports/install-readiness-latest.json`, and `reports/vendored-binary-latest.json` to the selected archive report allowlist.
- Covered idempotency (`--force` required for replacement), custom stdio, remote Streamable HTTP URL validation, disabled paid-server registration, and 100-server config-scale inventory.
- Wired install-scenario smoke into the install-readiness harness so the install path is checked alongside lifecycle, tool-safety, and upstream simulations.
- Did not bump the version because this pass added source/documentation/evidence around install scenarios without rebuilding the Rust binary; the vendored binary remains `0.6.2`.


## Fifth-pass changes

- Hardened the dashboard refresh loop against random tab switching, repeated manual refreshes, hidden-tab background churn, and out-of-order API responses.
- Replaced fixed `setInterval` polling with adaptive `scheduleAutoRefresh()` that re-arms after each refresh settles.
- Added `AbortController` plus monotonic `refreshSeq` stale-response guards.
- Added Page Visibility handling so hidden tabs pause normal polling and refresh when visible again.
- Made `/api/logs` failure degrade log rendering instead of blanking the whole overview.
- Added `scripts/dashboard-chaos-smoke.mjs`, `npm run verify:dashboard-chaos`, `npm run benchmark:dashboard-chaos`, and `npm run verify:experience`.
- Added `docs/dashboard-chaos-verification.md`, `reports/dashboard-chaos-smoke-latest.json`, `reports/dashboard-chaos-smoke-latest.md`, and `reports/dashboard-chaos-scenario-matrix-20260516.md`.
- Isolated package/vendor-mutating Node tests in `scripts/run-node-test-files.mjs` so `stage-vendored-binary`, `verify-npm-pack`, and `verify-vendored-binary` do not race with neighboring parallel batches.
- Did not bump the version because this pass changes source/docs/evidence without rebuilding the Rust binary; the vendored binary remains `0.6.2`.


## Sixth-pass changes

- Added `scripts/lifecycle-blast-radius-smoke.mjs`, `npm run verify:lifecycle-blast-radius`, `npm run benchmark:lifecycle-blast-radius`, and umbrella `npm run verify:hardening`.
- Added `docs/mcp-lifecycle-blast-radius.md` to document registered/disabled/enabled/tested/removed/replaced lifecycle states, paid/risky-server posture, ownership boundaries, and supply-chain package-manager risk.
- Added `reports/lifecycle-blast-radius-latest.json` and `reports/lifecycle-blast-radius-latest.md` to the selected archive report allowlist.
- Hardened Rust source loading so corrupt/unreadable unrelated MCP settings fragments are skipped with warnings instead of failing the whole registry/source report.
- Hardened Rust source replacement semantics so `--force` removes an existing normalized-match key before inserting the replacement, preventing case/punctuation-only duplicate drift in one fragment.
- Expanded Node runner isolation for spawn/package-heavy contract files after aggregate runs showed sandbox hangs when those tests shared neighboring parallel batches.
- Did not bump the package version because this pass changes source/docs/evidence without rebuilding the vendored Rust binary; the vendored binary remains `0.6.2`.


## Seventh-pass changes

- Added a real Playwright dashboard E2E lane through `scripts/playwright-dashboard-e2e.mjs` and `npm run verify:playwright-e2e`.
- The Playwright lane installs `@playwright/test` into a temporary npm prefix with browser download disabled, uses system Chromium when available, and leaves no project `node_modules` behind.
- Covered five real Chromium pages/tabs, search/filter interactions, manual refresh, hub-up action, content reload, synthetic slow overview responses, partial logs failure, and browser console-error fail-fast behavior.
- The sandbox Chromium has a managed URL blocklist that blocks direct local HTTP navigation, so the E2E test loads the dashboard HTML directly and mocks dashboard API responses in-browser via `window.fetch`. This still proves real browser DOM/event-loop/tab behavior, but it is not a full live Rust-dashboard HTTP test.
- Added `scripts/external-tool-internet-smoke.mjs`, `npm run verify:external-tool-internet`, `npm run verify:external-tool-internet:live`, and `npm run verify:live-experience`.
- Added `docs/browser-e2e-and-external-tooling.md` and `reports/external-tool-scenario-matrix-20260516.md` to cover Playwright/Puppeteer/Cypress-style browser tooling plus local-only, package-manager, container-runtime, external API, web-fetch, and remote MCP server scenarios.
- The source-only external-tool smoke passes without executing third-party MCP packages. The live-internet variant is explicit opt-in and only checks DNS/HTTPS reachability; in this sandbox it reports `blocked` because direct external HTTPS endpoints are unavailable.
- Added `tests/node/browser-e2e-and-external-tooling-contract.test.js` and added the new reports to the release manifest allowlist.
- Kept the version at `0.6.2` because this pass changes source/docs/evidence without rebuilding the vendored Rust binary; the vendored binary remains `0.6.2`.

## Verification performed in this packaging environment

Passed:

- `npm run verify:playwright-e2e`
- `npm run verify:external-tool-internet`
- `npm run verify:lifecycle-blast-radius`
- `npm run verify:hardening`
- `npm run verify:performance`
- `npm run verify:mcp-install-scenarios`
- `npm run verify:dashboard-chaos`
- `npm run verify:experience`
- `npm run verify:install-readiness`
- `npm run lint:npm`
- `npm run test:repo:smoke`
- `npm run test:npm`
- `npm run audit:source`
- Focused Node contracts for lifecycle/blast-radius, runtime performance, performance smoke, MCP install scenarios, docs, archive, evidence, stack/toolchain, product truth, eval, dashboard chaos, security, repo, and publish decision.
- Node repo contracts pass when run as shards/focused lanes in this sandbox; the newly added heavy tests are isolated by the runner to reduce parallel mutation/race risk.

Completed but correctly blocked:

- `npm run verify:publish-decision` completed and returned `blocked`, not release-ready, because Rust/build/release evidence is incomplete.

Blocked or not fully proven in this sandbox:

- `npm run verify:external-tool-internet:live` ran in opt-in live mode and produced an explicit `blocked` report because direct external HTTPS endpoints are unavailable from this host/network policy.
- A full Playwright run against a live Rust dashboard HTTP server was not performed because the sandbox Chromium has a managed URL blocklist that blocks direct local HTTP navigation; the added E2E lane instead uses real Chromium tabs with direct HTML loading and in-browser API mocks.
- `npm run verify:rust-quality` fails because `cargo` and `rustc` are unavailable.
- `npm run verify:local:source` and full aggregate `npm run test:repo` hit sandbox-level timeouts during long/heavy Node contract lanes, even though their constituent focused/sharded lanes completed successfully. This is recorded as environment limitation, not as release proof.
- `cargo fmt`, `cargo check`, `cargo test`, `cargo clippy`, and `cargo build --release` were not run.

Not run here:

- Full Playwright E2E against a compiled live Rust dashboard process over HTTP.
- Puppeteer/Cypress duplicate E2E lanes; they are documented as alternative popular tools, while Playwright is the default lane for multi-tab/network-mocking coverage.
- Real-client traces for Claude Desktop, Cursor, Windsurf, and other local clients.
- macOS/Windows/ARM native runtime execution.
- Live paid-provider MCP calls, third-party `npx`/`uvx` MCP package execution, Docker image execution, and npm publication/provenance proof.

## Performance posture notes

- `npm run verify:performance` produced a passing source-level smoke report in this sandbox.
- `npm run verify:dashboard-chaos` produced a passing source-level multi-tab/dashboard chaos report in this sandbox.
- The report measures benchmark wiring and synthetic boundedness, including tool-scale, mixed-upstream, and upstream-failsafe pressure.
- It does not prove final Rust binary latency, throughput, memory, or cross-platform behavior.
- Host-specific p50/p95/p99 and memory thresholds must be collected before using performance as a release gate.


## MCP install scenario posture notes

- `npm run verify:mcp-install-scenarios` produced a passing executable smoke report in this sandbox.
- Registration is config-only: `server install`/`server add` writes MCP settings fragments and defers package download, process launch, remote requests, and tool calls until later runtime/test/client execution.
- Reinstalling an existing normalized server name is blocked unless `--force` is used; `--force` replaces config, not an installed package.
- Remote URL domains are upstream/provider domains unless the user controls them. The MCPace endpoint domain is only the local/default or explicitly configured `serve.publicUrl`/`MCPACE_PUBLIC_MCP_URL`.
- 100 configured servers were covered as config-scale inventory, not as proof that 100 real paid/expensive servers can safely run concurrently.

## Security posture notes

- Upstream `stderr` diagnostics are bounded before being surfaced to users.
- Likely secrets in diagnostics are redacted, including bearer tokens and sensitive key/value assignments.
- Child-process runners use an explicit environment allowlist; registry credentials and sandbox secrets are not inherited by default.
- MCP HTTP requests are expected to carry `Content-Type: application/json` and the required Streamable HTTP `Accept` values.
- MCP HTTP session ids are generated from OS randomness and fail closed when OS entropy is unavailable.
- Non-loopback binds require explicit opt-in plus bearer-token authentication via `MCPACE_HTTP_AUTH_TOKEN` / `--auth-token-env`, unless the operator uses the deliberately named `--insecure-nonlocal-bind` lab-only escape hatch.

## Recommended final verification on a Rust host

```bash
cargo fmt --all -- --check
cargo check --all-targets --locked
cargo test --all-targets --locked
cargo clippy --all-targets --locked -- -D warnings
cargo build --release --locked
npm run verify:rust-quality
npm run verify:performance
npm run verify:dashboard-chaos
npm run verify:playwright-e2e
npm run verify:external-tool-internet
npm run verify:experience
npm run verify:mcp-install-scenarios
npm run verify:lifecycle-blast-radius
npm run verify:hardening
npm run verify:local-prepublish
npm run verify:publish-decision
```

## Basic local run from source

```bash
cargo build --release
./target/release/mcpace version
./target/release/mcpace serve
```

For npm launcher smoke after building:

```bash
MCPACE_BINARY_PATH=./target/release/mcpace node packages/npm/cli/bin/mcpace.js version
```

## 2026-05-16 parallel client/session and overhead pass

Changes:
- Added `scripts/overhead-audit.mjs` plus `npm run verify:overhead-audit` and `npm run benchmark:overhead-audit`.
- Added `tests/e2e/dashboard.parallel.playwright.spec.mjs` to run independent dashboard client sessions in parallel Playwright workers.
- Updated `tests/e2e/playwright.config.mjs` to use `fullyParallel: true`, configurable `MCPACE_PLAYWRIGHT_WORKERS`, and both tab-level and client-session specs.
- Updated `scripts/playwright-dashboard-e2e.mjs` to copy all E2E specs, collect runtime parallel-session evidence, and fail the report when checks fail.
- Added `reports/overhead-audit-latest.*` and `reports/playwright-parallel-session-matrix-20260516.md` to the source archive allowlist.
- Kept Playwright as an explicit browser lane instead of putting temporary package installs into every fast source verification.

Evidence in this sandbox:
- `npm run verify:playwright-e2e`: pass. Real Chromium ran 5 tests using 2 Playwright workers. Four independent client sessions produced no conflicts.
- `npm run verify:overhead-audit`: pass. Root dependency count 0; CLI runtime dependency count 0; Playwright remains test-only; dashboard source 32,365 bytes; npm launcher median delta was about 119 ms over the native binary on this host.
- `npm run verify:experience`: pass for source-level performance, dashboard chaos, and overhead audit.
- `npm run lint:npm`: pass, 129 checked files.
- `node --test tests/node/browser-e2e-and-external-tooling-contract.test.js`: pass, 5/5.
- `npm run test:npm`: pass, 3/3.
- `npm run audit:source`: pass, critical 0, warnings 3.

Limitations:
- The aggregate `npm run test:repo` was started and progressed through multiple batches, but the sandbox command timed out before completion. Focused updated contracts passed.
- Rust quality still cannot be proven here because this sandbox does not provide `cargo`/`rustc`.
- Live internet and real third-party MCP package execution remain opt-in only; this pass does not execute external MCP packages.

## 2026-05-16 multi-client runtime audit pass

Changes:
- Added `scripts/multi-client-runtime-audit.mjs`, `npm run verify:multi-client-runtime`, and `npm run benchmark:multi-client-runtime`.
- Wired `verify:multi-client-runtime` into `verify:experience` and `verify:browser-experience`.
- Added `docs/multi-client-runtime.md` plus explicit documentation in `docs/universal-runtime-policy.md` and `docs/browser-e2e-and-external-tooling.md`.
- Added `reports/multi-client-runtime-audit-latest.json` and `reports/multi-client-runtime-audit-latest.md` to the selected source archive allowlist.
- Added `tests/node/multi-client-runtime-contract.test.js` to keep the multi-client assumptions executable.
- Raised the automatic upstream session pool default from a single global shard to bounded multi-client sharding (`AUTO_UPSTREAM_SESSION_POOL_MAX=8`, `AUTO_UPSTREAM_SESSION_SHARD_MAX=4`) while retaining env overrides for host-specific tuning.
- Added an explicit Rust context warning for stdio clients that provide no session/conversation/client-instance/transport-session signal; MCPace can derive a stable planned lease, but strict multi-client isolation is not fully automatic in that case.

Evidence in this sandbox:
- `npm run verify:multi-client-runtime`: pass, 9/9 source checks.
- `npm run verify:browser-experience`: pass; Playwright E2E report shows 4 independent client contexts across 2 workers with zero conflicts.
- `npm run verify:experience`: pass; performance, dashboard chaos, overhead audit, and multi-client runtime audit all passed.
- `npm run lint:npm`: pass, 131 checked files.
- `npm run test:npm`: pass, 3/3 CLI test files.
- `npm run audit:source`: pass, critical 0, warnings 4.
- `node --test tests/node/browser-e2e-and-external-tooling-contract.test.js tests/node/multi-client-runtime-contract.test.js`: pass, 11/11.

Limitations:
- This is still not a Rust compile/runtime throughput proof. `cargo`/`rustc` are unavailable in this sandbox.
- Full Playwright against a live compiled Rust dashboard HTTP server remains live-host only.
- Real local clients may send different metadata quality. HTTP Streamable sessions are strong because the session id is server-issued and echoed back by the client; generic stdio clients without session metadata remain an explicit accepted limit.

## 2026-05-16 adaptive orchestration pass (v0.6.3)

- Added an evidence-driven adaptive orchestration layer on top of the existing `scopeClass`/`concurrencyPolicy` plan fields.
- Added `parallelSafetyClass`, `defaultPoolModel`, `workerPoolKey`, `maxWorkers`, `maxInFlightPerWorker`, `transportStatus`, `launcherKind`, `lockDomains`, and `profileEvidence` surfaces to the server/client planning model.
- Separated legacy SSE compatibility from stable Streamable HTTP: legacy `sse` now resolves as `sse-legacy`/`legacy-compat` and is never auto-parallelized by default.
- Added `scripts/adaptive-parallelism-audit.mjs`, `npm run verify:adaptive-parallelism`, and `npm run verify:orchestration`.
- Added schemas: `schemas/mcpace-server-profile.schema.json` and `schemas/mcpace-worker-plan.schema.json`.
- Added architecture doc: `docs/adaptive-mcp-orchestration.md`.
- Added regression coverage: `tests/node/adaptive-parallelism-contract.test.js`.
- Generated `reports/adaptive-parallelism-latest.json` and `reports/adaptive-parallelism-latest.md` and added them to the release manifest allowlist.

Proof run in this sandbox:

- `npm run lint:npm` passed, 133/133 checked files.
- `npm run verify:adaptive-parallelism` passed.
- `node --test tests/node/adaptive-parallelism-contract.test.js` passed, 4/4.
- `npm run test:npm` passed, 3/3.
- `npm run audit:source` passed with critical=0.

Still not proven here:

- Rust compile/fmt/clippy/tests, because this sandbox has no `cargo` or `rustc`.
- Live MCP server probes against third-party packages, because those can execute external code or spend money.
- Full live-browser E2E against a compiled Rust dashboard server, because it still requires a Rust host and a browser environment without this sandbox's managed URL restrictions.


## 2026-05-16 adaptive edge-case coverage pass (v0.6.4)

Changes:
- Bumped the source snapshot metadata to `0.6.4` across Rust/npm manifests, platform package manifests, `mcpace.config.json`, and current product-truth metadata.
- Expanded `scripts/adaptive-parallelism-audit.mjs` from preset-only checks to an explicit synthetic edge-case matrix.
- Added `docs/adaptive-edge-case-coverage.md`.
- Extended `tests/node/adaptive-parallelism-contract.test.js` from 4 to 5 tests.

Edge cases now covered by source-level adaptive audit:
- unknown stdio package (`npx`), legacy SSE, remote Streamable HTTP, credential-scoped API, project-local filesystem, repo/git, browser automation, shared-exclusive desktop state, read-only stdio candidate, and unknown OCI/container launcher.
- Every edge-case classification must carry a lock or scheduling domain.
- Unknown/high-risk stdio remains `maxInFlightPerWorker=1` by default.

Proof run in this sandbox:
- `npm run verify:adaptive-parallelism`: pass, 4 runtime/config profiles plus 10 synthetic edge cases.
- `node --test tests/node/adaptive-parallelism-contract.test.js`: pass, 5/5.
- `npm run verify:orchestration`: pass.
- `npm run verify:hardening`: pass.
- `npm run verify:browser-experience`: pass after increasing the wrapper timeout; real Chromium + Playwright ran independent contexts/workers.
- `npm run test:repo:smoke`: pass, 7/7 selected node contracts.
- `npm run test:npm`: pass, 3/3.
- `npm run audit:source`: pass with critical=0.
- `node --test tests/node/archive-contract.test.js`: pass.

Still not proven here:
- Rust compile/fmt/clippy/tests and rebuilt `0.6.4` native binary, because this sandbox has no `cargo` or `rustc`. The vendored binary artifact is still older and must not be used as release proof for `0.6.4`.
- Live third-party MCP package probes and paid-provider calls, because they can execute external code or spend money.
- Full live Rust-dashboard HTTP E2E on a real host.


## 2026-05-16 adaptive worker-plan materialization pass (v0.6.5)

Changes:
- Bumped the source snapshot metadata to `0.6.5` across Rust/npm manifests, platform package manifests, `mcpace.config.json`, selected docs, and current product-truth metadata.
- Added `scripts/adaptive-worker-plan.mjs`, `npm run verify:adaptive-worker-plan`, and `npm run benchmark:adaptive-worker-plan`.
- Updated `npm run verify:orchestration` so it verifies adaptive classification, worker-plan materialization, multi-client runtime assumptions, lifecycle/blast-radius, and performance in one source-level lane.
- Added `docs/adaptive-worker-plan.md`.
- Extended `schemas/mcpace-worker-plan.schema.json` so generated worker plans explicitly include `source` and `parallelSafetyClass` while keeping additional properties disallowed.
- Extended `tests/node/adaptive-parallelism-contract.test.js` to assert concrete worker plans for runtime profiles and synthetic edge cases.
- Added `reports/adaptive-worker-plan-latest.json` and `reports/adaptive-worker-plan-latest.md` to the selected source archive allowlist.
- Removed the remaining source-audit warnings from helper scripts that used explicit `process.exit(0)` for help/success paths.

What the new worker-plan lane proves:
- Runtime profiles and synthetic edge cases materialize into concrete scheduler decisions.
- Unknown stdio and OCI/container launchers stay one in-flight per worker until safe probes pass.
- Legacy SSE stays `legacy-disabled` with zero workers and zero in-flight capacity.
- Remote Streamable HTTP carries transport/session and credential/provider budget affinity.
- Credential-scoped APIs carry credential/session affinity and auth-mixup degradation.
- Project/file/repo tools carry project/resource write locks.
- Browser automation carries browser-context/session affinity and consent/review gates.
- Every generated plan carries conflict, crash-loop, auth-mixup, and latency-regression degradation policies.

Proof run in this sandbox:
- `npm run verify:adaptive-parallelism`: pass, 4 runtime profiles plus 10 synthetic edge cases.
- `npm run verify:adaptive-worker-plan`: pass, 14 concrete worker plans, 0 blockers.
- `node --test tests/node/adaptive-parallelism-contract.test.js`: pass, 6/6.
- `npm run verify:orchestration`: pass.
- `npm run lint:npm`: pass, 134/134 JS/MJS files.
- `npm run test:repo:smoke`: pass, 7/7 selected contracts.
- `npm run test:npm`: pass, 3/3 CLI test files.
- `npm run audit:source`: pass with critical=0 and warnings=0.
- `npm run verify:secrets`: pass, findings=0.
- `npm run verify:supply-chain`: pass-with-warnings, blockers=0; external tools like cargo-audit/cargo-deny/gitleaks/osv-scanner/trivy are unavailable in this sandbox.
- `npm run verify:runtime-trace`: pass using the older vendored binary, enough to keep the smoke shape visible but not enough for a `0.6.5` release proof.
- `npm run verify:publish-decision`: correctly blocked.

Still not proven here:
- Rust compile/fmt/clippy/tests and rebuilt `0.6.5` native binary, because this sandbox has no `cargo` or `rustc`.
- The vendored native binary inside the archive is still older (`0.6.2`) and must not be used as release proof for `0.6.5`.
- `npm run verify:local:source` timed out in this sandbox before writing its report; targeted source/orchestration lanes above passed.
- Live third-party MCP package probes, paid-provider calls, and full live Rust-dashboard HTTP E2E remain live-host tasks.
