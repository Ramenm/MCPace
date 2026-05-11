# Bug hunting and fix playbook

MCPace should treat every defect as a small engineering investigation, not as a one-line patch. A good fix leaves the product easier to reason about than before the bug was found.

## The defect lifecycle

### 1. Intake and classify

Every bug report should name the area, severity, platform, version or commit, client surface, upstream server type, and whether it is a regression. Use the taxonomy in `docs/defect-taxonomy-and-labels.md` so maintainers can search, route, and compare issues.

Severity guide:

- **S0 security/data-loss:** exposed secrets, unsafe public bind, arbitrary command execution, broken rollback, corrupted user config.
- **S1 broken core path:** `serve`, `/mcp`, `client install`, `server test`, runtime trace, or npm launcher cannot complete on a supported host.
- **S2 compatibility/runtime bug:** specific client, upstream, platform, or protocol edge fails while the main path still works.
- **S3 paper cut:** confusing output, docs drift, minor UX, slow but bounded operation.

### 2. Reproduce before changing code

Capture the smallest failing case:

```bash
npm run lint:npm
npm run verify:bug-sweep
npm run audit:source
npm run test:repo -- --timeout-ms 180000
npm run test:npm
npm run verify:product-practice
```

For Rust/runtime bugs, capture the exact host and run the narrowest command first:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
cargo build --release --locked
npm run verify:runtime-trace
```

For client/upstream bugs, record the exact client, upstream entry, `mcpace connect --json`, `mcpace server sources --json`, and `mcpace server test <name> --refresh --json`. Redact tokens, private paths, and env values before pasting anything into GitHub.

### 3. Minimize and isolate

Reduce the failure until one invariant is broken. Good minimal cases usually fit one of these forms:

- one JSON config fragment under `mcp_settings.d/`;
- one HTTP request/response transcript for `/mcp`;
- one stdio fixture server;
- one client config backup/diff fixture;
- one release/packaging manifest fixture;
- one failing contract assertion.

Do not start by editing broad modules. First identify the boundary: CLI parsing, config loading, client patching, HTTP boundary, session lifecycle, upstream process launch, lease ownership, report generation, package launcher, or release workflow.

### 4. State the root cause

Before the fix, write a root-cause sentence in the PR:

```text
Root cause: <invariant> was not enforced at <boundary>, so <input/state> produced <bad behavior>.
```

Examples:

- `Root cause: /mcp trusted a client-supplied session id during initialize, so a caller could force predictable session ids instead of receiving a server-minted cryptographic id.`
- `Root cause: runtime proof accepted stale reports, so product-practice could overclaim beta readiness without a fresh host-compatible trace.`
- `Root cause: server source merging treated a disabled fragment as absent, so diagnostics hid the disabled server instead of explaining it.`

### 5. Add the regression test first or with the fix

A bug fix should normally include one of:

- Rust unit/integration test for protocol/runtime behavior;
- Node contract test for repo, docs, package, or proof harness behavior;
- fixture-level test for config/client export/manifest behavior;
- runtime trace fixture or real-client proof when the bug is only visible end-to-end.

If a regression test is impossible in the current environment, the PR must say why and include a manual proof command/output.

### 6. Fix at the boundary, not only at the symptom

Prefer fixes that enforce invariants where data enters the system:

- validate request headers before routing;
- normalize and bound config input before merging;
- sanitize child process environment before spawning;
- mint session identifiers server-side;
- bind runtime claims to fresh reports;
- reject unsupported transports with explicit diagnostics;
- keep rollback data before mutating user config.

Avoid fixes that only hide the error message, skip tests, increase timeouts without diagnosis, or add a special case for one client when the boundary rule is wrong.

### 7. Verify and record evidence

A ready bug-fix PR should include:

```text
Reproduction before fix: <command / request / fixture>
Root cause: <one sentence>
Fix: <boundary invariant enforced>
Regression test: <test name or manual proof>
Verification: <commands and status>
Not tested: <honest gaps>
```

For release/runtime claims, prefer generated reports under `reports/` over prose. Product claims should not advance from preview to beta unless the relevant report is fresh, host-compatible, and passing.

### 8. Roll out safely

For changes touching install, user config, release, or runtime sessions:

- keep dry-run and diff paths working;
- preserve backup/restore behavior;
- reject unsafe states by default;
- add a compatibility note when behavior becomes stricter;
- keep the previous diagnostic message discoverable when possible;
- update README/ROADMAP/docs if the public contract changed.

### 9. Learn from repeated bugs

If the same class of bug appears twice, add automation:

- a new `verify:bug-sweep` rule;
- a contract test;
- a schema constraint;
- a stronger issue template field;
- a CI job or report freshness gate;
- an ADR if the fix changes architecture.

## What “fixed” means

A bug is fixed only when all of these are true:

1. The failing behavior is reproduced or convincingly reconstructed.
2. The root cause is stated in terms of a broken invariant.
3. The boundary now enforces the invariant.
4. A regression test or proof artifact exists.
5. The docs/product claims still match reality.
6. The fix does not weaken security, rollback, or release proof.

## Team roles for serious defects

For S0/S1 issues, split the work as if a small team is present:

- **Incident owner:** keeps the issue focused and records the current hypothesis.
- **Runtime owner:** checks `/mcp`, sessions, leases, upstreams, cancellation, and backpressure.
- **Security owner:** checks Origin/Host, tokens, env isolation, SSRF, path traversal, and secret redaction.
- **QA owner:** builds the smallest regression and verifies the full command matrix.
- **Release owner:** checks npm/platform artifacts, reports, provenance, and public claims.
- **DevRel owner:** updates README, docs, issue templates, and migration notes.

One person can hold multiple roles, but the checklist should still be explicit.
