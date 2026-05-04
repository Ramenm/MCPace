# MCPace bug lifecycle and fix standard

MCPace is a local MCP hub/control-plane. Bugs can appear in CLI parsing, client configuration patching, `/mcp` Streamable HTTP behavior, upstream stdio process handling, release packaging, or proof reports. This document defines how maintainers find, classify, fix, and prove bug fixes before public release claims.

## Non-negotiable fix standard

Every non-trivial bug fix should move through this path:

1. **Intake**: capture version/commit, platform, area, severity, expected behavior, actual behavior, reproduction, and redacted logs.
2. **Reproduce**: run the exact user path or reduce it to a minimal failing test, fixture, or runtime trace.
3. **Classify**: assign `type:*`, `area:*`, `severity:*`, and `status:*` labels.
4. **Root cause**: identify the smallest incorrect assumption, state transition, config merge rule, transport rule, or release-proof mismatch.
5. **Minimal failing test**: add a test or harness assertion that fails before the fix. For runtime bugs, use a client -> MCPace -> upstream trace where possible.
6. **Fix**: change the smallest safe unit. Avoid broad rewrites unless the root cause is architectural.
7. **Regression guard**: keep the failing test/trace in the suite and make it deterministic.
8. **Proof**: run the relevant quality gates and save fresh reports when the change affects runtime, release, install, or GitHub readiness.
9. **Release note**: document user-visible fixes, affected versions, and any behavior change.

A bug is not considered closed because the immediate symptom disappeared. It is closed when the root cause is understood and the regression guard would catch the same class of failure again.

## Severity model

| Label | Meaning | Examples |
|---|---|---|
| `severity:p0` | Security, data loss, or core runtime dead | session hijack risk, leaked secret, release binary launches wrong executable |
| `severity:p1` | Main product workflow broken | `/mcp` initialize/tools/list broken, stdio upstream calls fail for normal config |
| `severity:p2` | Degraded workflow or confusing behavior | stale report gives wrong readiness, client install diff is misleading |
| `severity:p3` | Polish/docs/rare edge | typo, unclear help text, non-critical UI copy |

Security-sensitive reports should not be triaged in public issues. Use the private security report path described in `SECURITY.md`.

## Areas to triage against

| Area | What to inspect first |
|---|---|
| `area:mcp-http` | `src/dashboard/mcp_http.rs`, `src/dashboard/http_boundary.rs`, `src/dashboard/http_headers.rs`, `src/dashboard/http_session.rs`, `docs/mcp-http-api-spec.md` |
| `area:upstream-stdio` | `src/upstream/stdio_runtime.rs`, `src/upstream/lease_runtime.rs`, upstream fixtures, stderr redaction |
| `area:upstream-http` | inventory/diagnostics paths and future HTTP fan-out connector code |
| `area:client-config` | `src/client/actions/*`, client catalog, backup/restore behavior |
| `area:npm` | `packages/npm/cli`, platform package manifests, binary resolver, npm pack output |
| `area:release` | release matrix, checksums, provenance, staged binary reports |
| `area:docs` | README, product truth, roadmap, public claims vs proof reports |

## How to find bugs proactively

Run these from cheapest to most behavioral:

```bash
npm run lint:npm
npm run audit:source
npm run verify:defect-gates
npm run verify:github-readiness
npm run verify:install-readiness
npm run verify:product-practice
npm run verify:runtime-trace
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
cargo build --release --locked
```

Use targeted probes when the report mentions a specific surface:

```bash
npm run test:repo
npm run test:npm
npm run verify:npm-pack
npm run verify:platform-packages
npm run verify:vendored-binary
npm run verify:rust-quality
```

For user-reported MCP runtime bugs, capture the smallest trace with:

```text
client -> http://127.0.0.1:<port>/mcp -> initialize -> tools/list -> tools/call -> upstream result
```

The trace should include exact request method names, negotiated protocol version, whether `Mcp-Session-Id` was issued and reused, selected upstream server, and redacted tool result.

## MCP-specific bug checklist

When touching `/mcp` behavior, verify:

- initialize negotiates a supported protocol version;
- server-generated `Mcp-Session-Id` is returned and later required;
- missing session headers return `400 Bad Request` after initialize;
- unknown or closed sessions return `404 Not Found`;
- `DELETE /mcp` closes an existing session;
- Origin guard accepts only loopback web origins and rejects `null`, `file://`, userinfo, and host-suffix tricks;
- local serve/dashboard does not bind non-loopback hosts unless the operator explicitly opts in;
- standard MCP headers and body metadata cannot contradict each other;
- malformed JSON-RPC returns protocol errors instead of panics.

When touching upstream behavior, verify:

- child process env is source-only and secret-redacted;
- command/cwd resolution is deterministic;
- diagnostics are bounded;
- stateful sessions are keyed by project/client/session context;
- stale lease/session results cannot be confused with a new call;
- HTTP upstream entries stay explicit diagnostics until HTTP fan-out is implemented.

When touching release/install behavior, verify:

- source package and platform package versions match;
- platform package manifests map to the generated release matrix;
- native binaries are host/target compatible before install-readiness claims;
- stale reports cannot satisfy product-practice gates;
- npm packaging stays thin-launcher unless binaries are actually staged.

## Fix review checklist

Before merging a bugfix PR, reviewers should ask:

- Is the bug reproduced in a deterministic test, fixture, or runtime trace?
- Is the root cause explained in one or two sentences?
- Is the fix scoped to the root cause rather than hiding the symptom?
- Would the regression guard fail if the old bug returned?
- Are security/privacy implications considered?
- Are reports regenerated when the public claim changes?
- Is every untested lane listed in `Not-tested`?

## Release verification after fixes

For public release or GitHub launch work, a fix is release-ready only when fresh reports exist for the affected surface. Runtime fixes need runtime trace proof. Packaging fixes need npm/platform proof. Security fixes need private disclosure handling, redaction proof, and a release note.
