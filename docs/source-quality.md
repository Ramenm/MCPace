# Source quality audit

MCPace now has a lightweight source-audit lane for architectural risk signals
that are useful before a full Rust host build is available:

```bash
npm run audit:source
node scripts/audit-source.mjs --json --write reports/source-audit-latest.json
```

The audit is intentionally not a subjective style linter. It separates findings
into two classes:

- **Critical:** deterministic production-code hazards that should block CI, such
  as production Rust `todo!`, `unimplemented!`, or `panic!` macros, plus explicit
  architecture-boundary violations.
- **Warnings:** refactor-planning signals, such as large module files, direct
  thread spawns outside reviewed runtime fan-out modules, production `unwrap()`
  counts, and script patterns worth revisiting when architecture work continues.

Warnings do not fail the command because this repository still has known large
modules (`dashboard`, `upstream`, `client/actions`, and protocol/tooling
surfaces) that are better split after the behavior contracts are fully locked
down. Treat warning counts as an architecture backlog, not as a release blocker.

## Architecture boundaries covered now

The audit currently checks two high-value boundaries:

1. `src/mcp_protocol.rs` must stay command-, transport-, and runtime-agnostic.
   It may define JSON-RPC/MCP protocol primitives, but it must not spawn
   commands, open sockets, call the CLI router, or depend on runtime state.
2. `src/resources.rs` must stay side-effect free. It may compute resource
   defaults and limiter state, but it must not shell out, open sockets, or read
   and write project state.

These checks are deliberately small. New boundaries should be added only when the
rule is objective enough that a false positive would be rare.

## How to use the report

- Run `npm run audit:source` during local cleanup and in source CI.
- Use `--json --write reports/source-audit-latest.json` when a pass needs a
  durable artifact for reports or release notes.
- When a warning stays stable for several releases, either split the module or
  explicitly document why the coupling remains acceptable.
- Keep critical checks small. If the list becomes subjective, it will create
  false failures and people will ignore it.

## Current architecture note

The local HTTP adapter now has a route-level error boundary. Internal command
failures are converted into a structured JSON `500 Internal Server Error` payload
with `ok=false` and an `internal_error` code before the request is marked failed
for runtime counters. That keeps clients from seeing a silent socket close when
an internal command fails and makes dashboard/API failure modes testable.


## Unsafe and FFI boundary

The only approved production Rust unsafe/FFI boundary files are now:

- `src/process_detach.rs` for Unix `setsid`/`pre_exec` background launch setup;
- `src/windows_process.rs` for Windows hidden detached process creation.

`src/hub/launcher.rs` and `src/serve.rs` call the reviewed helper instead of
embedding their own unsafe blocks. `scripts/audit-source.mjs` counts unsafe
operations and foreign-function blocks and treats any new production unsafe/FFI
outside the approved boundary files as a critical finding.

## Rust host quality gate

A separate Rust host gate is available for environments with Cargo installed:

```bash
npm run verify:rust-quality
```

That script writes `reports/rust-quality-latest.json` and runs the lanes in a
fixed order: `cargo fmt --all -- --check`, `cargo clippy --all-targets --locked -- -D warnings`,
`node scripts/run-rust-tests.mjs --json --profile non-lifecycle`, and
`cargo build --release --locked`. In constrained environments, use
`node scripts/verify-rust-quality.mjs --json --allow-missing-cargo` only to
produce an honest partial report; do not treat that as build proof.
