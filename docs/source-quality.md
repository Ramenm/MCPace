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

Warnings do not fail the command because they are refactor-planning signals, not always correctness defects. After the v0.5.5 modularization pass, the current source audit reports zero production large-module warnings; if warnings return, treat them as an architecture backlog, not as a release blocker unless a critical boundary is also violated.

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


## v0.5.5 module split update

The source audit now treats `src/**/tests.rs` files as test modules. This keeps extracted Rust tests from being counted as production code after the dashboard/upstream/adapter/MCP-server test modules were moved out of their parent files.

After the v0.5.5 split, source audit reports zero production large-module warnings. Further splitting should be behavior-driven and should wait for a Cargo check/test gate when it touches high-coupling runtime behavior.

## v0.5.5 adapter boundary update

The adapter root now has two additional focused child modules:

- `src/adapter/profile.rs` for adapter profile rendering and initialize-derived client capability summaries.
- `src/adapter/proxy_uri.rs` for proxied upstream resource URI encoding/decoding and upstream error metadata helpers.

A source-quality contract now verifies this split and checks that helper functions used by the adapter root from `discovery.rs` are explicitly `pub(super)`. This is a source-level guard against module extraction drifting into Rust visibility errors before a full Cargo check is available.


## v0.5.5 catalog/stdio boundary update

The source-quality contract now also guards two additional module boundaries:

- `src/client_catalog/builtin.rs` owns static built-in client defaults; `src/client_catalog.rs` owns registry loading, merge behavior, and selector resolution.
- `src/mcp_server/args.rs` owns stdio MCP argv parsing/help; `src/mcp_server.rs` owns JSON-RPC lifecycle and command dispatch.

`scripts/lib/client-catalog.mjs` reads the built-in catalog file first and retains old-location fallback only for transition compatibility. This prevents test/report tooling from silently drifting when catalog defaults are moved to a focused boundary.


## v0.5.5 client action backup boundary update

Client install backup/restore helpers now live in `src/client/actions/backup.rs`. The source-quality contract checks that `src/client/actions.rs` declares `mod backup`, imports backup helpers through that child module, and stays under the client action dispatcher line-count target. Keep future mutation-support helpers in focused child modules instead of expanding the dispatcher root.


## v0.5.5 client-first connect boundary

`mcpace connect` is a read-only orchestration command. Its implementation is split across `src/connect.rs` and focused `src/connect/*` modules, and Node contract tests assert that it uses existing read paths while avoiding MCP settings and client-config mutation helpers.


## v0.5.9 server preset rendering boundary

Preset-specific text/JSON rendering now lives in `src/server/preset_render.rs`. The generic `src/server/render.rs` stays focused on configured-server list, capability, test, remove, and toggle output. A source-quality contract checks this boundary so useful-MCP onboarding can grow without turning the generic server renderer back into a mixed command surface.
