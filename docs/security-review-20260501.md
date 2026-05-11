# Security review: 2026-05-01

## Scope

Reviewed source archive paths relevant to local MCP operation, release hygiene,
and diagnostic safety:

- `src/dashboard.rs`
- `src/upstream.rs`
- `mcp_settings.json`
- `mcpace.config.json`
- `.gitignore`
- `release-manifest.json`
- `tests/node/security-contract.test.js`
- `reports/verification-latest.json`

## Findings

### Medium — required source context was ignored by `.gitignore`

Where: `.gitignore`, `release-manifest.json`, `tests/node/security-contract.test.js`

Risk: clean source archives and local checkouts could diverge. The manifest and
test suite required `memory-bank/`, but `.gitignore` excluded it. That caused
source proof failure and made security posture documentation non-reproducible.

Recommendation: treat `memory-bank/` as tracked source context or remove it from
the manifest/tests. This session chose tracked source context and created the
required files.

### Medium — `/mcp` invalid Origin needed route-level 403 handling

Where: `src/dashboard.rs`

Risk: invalid browser origins should be rejected as transport/security failures,
not mixed with JSON-RPC parse/validation behavior. Without route-level handling,
the observable status could be less clear and less aligned with local HTTP
security policy.

Recommendation: reject forbidden origins before MCP request handling for both
GET and POST.

### Low/Medium — unsupported SSE GET lacked `Allow: POST`

Where: `src/dashboard.rs`

Risk: clients cannot distinguish "wrong method, use POST" from other unsupported
method cases as cleanly. This weakens diagnostics and protocol hygiene.

Recommendation: return `405 Method Not Allowed` with `Allow: POST`.

### Medium — POST `/mcp` accepted missing Streamable HTTP `Accept` contract

Where: `src/dashboard.rs`

Risk: clients that are not prepared to handle both JSON and SSE response modes
can connect successfully but fail later or hide compatibility bugs.

Recommendation: require both `application/json` and `text/event-stream` in POST
`Accept`.

### Resolved/monitored — large modules are now below the source-audit threshold

Where: `src/upstream.rs`, `src/dashboard.rs`, `src/adapter.rs`,
`src/client/actions.rs`, `src/mcp_server.rs` and their child modules.

Risk if it regresses: high-change critical paths become harder to review,
especially where they mix routing, transport, process spawning, diagnostics,
sessions, and tests.

Current status: the v0.5.5 split moved MCP HTTP, upstream stdio runtime,
lease/session/cache helpers, adapter discovery, client render/config helpers,
and tool-surface tests into focused child modules. Source audit now reports zero
production large-module warnings.

Recommendation: keep monitoring this with source audit. Do not split further
only for line count; split only when a behavior boundary is changing and Cargo
check/test/build is available.

## Patch summary

- Created `memory-bank/` source files.
- Removed `/memory-bank/` ignore rule.
- Hardened `/mcp` GET/POST route behavior.
- Added integration-style Rust test coverage for cross-origin and missing
  `Accept` cases.
- Added `Allow: POST` assertion to the existing unified serve route test.

## How to verify

Executed in this sandbox:

```bash
node --test tests/node/*.test.js packages/npm/cli/test/*.test.mjs
node scripts/audit-source.mjs --json
node scripts/build-release-artifacts.mjs --json
```

Still required on a Rust-enabled host:

```bash
cargo fmt --all -- --check
cargo test --all-targets --locked
npm run verify:rust-quality
```

## НЕ ПОДТВЕРЖДЕНО

- AuthN/AuthZ behavior for remote HTTP operation.
- Stateful `MCP-Session-Id` implementation.
- Real-host tier-1 client traces for upstream tool calls.

## Additional hardening pass: 2026-05-01

### Medium — optional `Mcp-Method` / `Mcp-Name` headers were not checked against the JSON-RPC body

Where: `src/dashboard.rs`, `docs/mcp-http-api-spec.md`

Risk: a client or intermediary can place one operation name in the HTTP headers
and a different operation name in the JSON-RPC body. Future MCP draft guidance
explicitly treats this as a header/body source-of-truth problem. Even before
strict standard-header mode, mismatch detection is useful because MCPace is a
local routing boundary.

Recommendation: keep backwards compatibility for clients that omit those
headers, but reject mismatches with `400 Bad Request`. This pass implemented
that compatibility hardening for `Mcp-Method` and for `Mcp-Name` on
`tools/call`, `prompts/get`, and `resources/read`.

### Medium — source/release proof child processes inherited the full parent environment

Where: `tests/node/helpers.js`, `scripts/archive-release.mjs`,
`scripts/proof-report.mjs`, `scripts/run-rust-tests.mjs`,
`scripts/verify-rust-quality.mjs`, npm/package verification scripts

Risk: source proof and release verification subprocesses did not need registry,
package-index, sandbox, or agent context variables, but could inherit them from
the shell. That increases accidental credential exposure through child process
crashes, debug output, or external tools.

Recommendation: use a shared explicit allowlist. This pass added
`scripts/lib/safe-child-env.mjs`, kept the test helper aligned, and limited npm
publish credentials to the publish script where they are intentional.

### Low/Medium — Node source tests needed deterministic process isolation in this sandbox

Where: `package.json`, `tests/node/coverage-contract.test.js`

Risk: repository contract tests launch child processes. In this sandbox, a plain
parallel `node --test` run could print passing TAP output and still keep the
runner alive. That makes local source proof flaky.

Recommendation: run Node source/npm tests through the per-file `scripts/run-node-test-files.mjs` wrapper. The wrapper invokes each file with `node --test --test-force-exit`, favoring deterministic source proof over marginal parallel speed.

## Additional verification

Executed in this sandbox after the additional hardening pass:

```bash
node --test --test-force-exit tests/node/security-contract.test.js
node --test --test-force-exit tests/node/coverage-contract.test.js
node scripts/audit-source.mjs --json
```

Still required on a Rust-enabled host:

```bash
cargo fmt --all -- --check
cargo test --all-targets --locked
npm run verify:rust-quality
```

## v0.5.5 follow-up: session-id echo and registry drift

### Finding: Streamable HTTP session id echo needed explicit normalization

- Severity: Medium.
- Where: `src/dashboard.rs`, initialize response headers.
- Risk: a client-controlled `Mcp-Session-Id` value was echoed after trimming. HTTP parsing already removes line boundaries, but explicit visible-ASCII validation is the safer boundary because MCP session ids are response headers and the MCP transport spec constrains session IDs to visible ASCII.
- Remediation: `normalize_mcp_http_session_id` now accepts only non-empty visible ASCII values bounded by `resources::MAX_HTTP_HEADER_LINE_BYTES`; otherwise MCPace generates a local id. Generated ids use cross-platform OS randomness via `getrandom` when available and use an explicit `mcpace-fallback-` prefix for fallback ids.
- Verification: `tests/node/security-contract.test.js` includes a source contract for normalization and OS-random generation path. `npm test` passes.

### Finding: doctor/readiness did not use the multi-source MCP registry

- Severity: Medium.
- Where: `src/doctor.rs`.
- Risk: upstreams loaded via `mcpace.config.json -> mcpSettings.includePaths` or `MCPACE_MCP_SETTINGS` could be routable by runtime paths but invisible to runtime prerequisite diagnostics.
- Remediation: `load_source_runtime_commands` now calls `mcp_sources::load_mcp_server_registry(root_path)`, matching `src/upstream.rs` and `src/server/loader.rs`.
- Verification: `tests/node/configurable-mcp-connectivity-contract.test.js` asserts doctor registry parity. `npm test` passes.

## v0.5.5 convenience security follow-up

- `server add` validates `--env` keys as environment-variable names before writing JSON fragments.
- `server add` validates `--header` keys before writing remote/HTTP inventory entries.
- `--env` and `--header` values reject NUL, carriage return, and newline characters, reducing accidental header/env injection through generated settings fragments.
- `server add --url` is registry/inventory-ready only; remote HTTP upstream forwarding is still not implemented and should not be treated as a completed auth/SSRF boundary.
- Node source test runner now avoids using `--test-force-exit` on Node versions where the flag is unsupported, reducing false local proof failures.
