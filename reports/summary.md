# MCPace v0.5.9 summary

## Current state

MCPace is now healthier as a source/thin-launcher/native-BYO-MCP project, but it is still not fully runtime/beta ready. This pass focused on the higher-level practice problem: the project was getting many features, reports, and commands, but still needed a stricter gate that prevents claiming runtime or published-install readiness before the real broker loop is proven.

## What changed in v0.5.9

### Product-practice and runtime-proof guardrails

New proof lanes:

```bash
npm run verify:product-practice
npm run verify:runtime-trace
```

They produce:

```text
reports/product-practice-latest.json
reports/product-practice-latest.md
reports/runtime-trace-latest.json
reports/runtime-trace-latest.md
```

The product-practice harness separates these claims:

```text
source tree healthy
thin npm launcher usable
runtime beta ready
published binary install ready
universal remote MCP broker ready
```

Only the first two are currently allowed. Runtime beta, published binary install, and universal remote MCP brokering remain blocked until the corresponding proof exists.

### Security posture guardrails

Upstream process stderr is treated as diagnostic evidence, not as trusted output:
MCPace keeps stderr snippets bounded and redacts likely secrets before surfacing
them in user-facing errors. Child-process proof lanes also use explicit env
allowlisting through `scripts/lib/safe-child-env.mjs` so registry credentials,
sandbox tokens, and unrelated host environment variables are not forwarded by
default.

### Node source checking is no longer a hardcoded package.json list

`lint:npm` now runs one auto-discovery harness:

```bash
node scripts/check-node-syntax.mjs --json
```

The previous practice of maintaining a long `node --check file && node --check file ...` command in `package.json` was brittle. New JS/MJS files under `packages/npm/cli`, `scripts`, `tests/node`, `tests/fixtures`, and `examples` are discovered automatically.

### Runtime trace fixture groundwork

Added:

```text
tests/fixtures/tiny-mcp-stdio-server.mjs
```

It implements a tiny deterministic stdio MCP server with:

```text
initialize
tools/list
tools/call -> tiny_echo
```

The runtime-trace harness now has the upstream fixture it needs. The remaining runtime-trace blocker is the compiled/staged MCPace binary plus a real client or inspector trace.

### Start-here path

Added/updated:

```text
START-HERE.md
docs/product-practice.md
```

`START-HERE.md` is now included in `release-manifest.json`, so clean archives keep the top-level operating order.

## Current inventory

From `reports/code-inventory-latest.json` / `reports/code-inventory-20260502.json`:

```text
total files:       454
Rust files:        128
Node JS/MJS files: 80
Markdown files:    119
JSON files:        91
test files:        49
docs files:        71
reports files:     54
schema files:      2
```

Source audit remains clean:

```text
critical: 0
warnings: 0
largeModules: 0
productionUnwraps: 0
```

## Verified in this environment

- `cargo fmt --all -- --check` — PASS.
- `npm run lint:npm` — PASS, `80/80` JS/MJS files checked.
- Repo Node tests were covered in split runs: the first sequential `npm run test:repo` run passed files through `platform-packages-contract.test.js`, and the remaining repo test files were rerun in one grouped `node --test` command with `68/68` tests passing. A single uninterrupted `npm run test:repo` run still timed out in this sandbox, so do not call it a single-run full-suite proof here.
- `npm run test:npm` — PASS, `3/3` npm CLI test files.
- `node scripts/audit-source.mjs --json --write reports/source-audit-latest.json` — PASS.
- `node scripts/verify-npm-pack.mjs --json` — PASS for `@mcpace/cli@0.5.9` thin launcher.
- `node scripts/boot-harness.mjs --json --write reports/boot-harness-latest.json --markdown reports/boot-harness-latest.md` — PASS with install readiness `partial` in this environment.
- `node scripts/install-readiness-harness.mjs --json --write reports/install-readiness-latest.json` — PASS with public status `ready-with-warnings`.
- `node scripts/product-practice-harness.mjs --json --write reports/product-practice-latest.json --markdown reports/product-practice-latest.md` — PASS with status `prove-rust-before-runtime-claims`.
- `node scripts/runtime-trace-harness.mjs --json --write reports/runtime-trace-latest.json --markdown reports/runtime-trace-latest.md` — PASS as a harness, status `blocked` because no compiled/staged binary is present.

## Blocked / not verified

- `cargo check --all-targets --locked` is blocked by crates.io DNS/dependency access in this environment.
- Full Rust `cargo test` and `cargo build --release` are not confirmed in this sandbox.
- A full end-to-end real-client runtime trace is not confirmed: `client -> /mcp -> initialize -> initialized -> tools/list -> tools/call -> real stdio upstream`.
- Durable HTTP session store is still not implemented.
- Remote Streamable HTTP upstream forwarding is still not implemented as a callable path; remote entries are registry/inventory entries only.
- Published npm install readiness still needs staged native binaries or platform binary packages; current npm package mode is a thin launcher.

## Current technical-debt priority

1. Run full Cargo check/test/build with dependency access.
2. Stage at least one native binary/platform package before claiming published npm install readiness.
3. Run the runtime-trace harness again after the binary exists; then record one real-client MCP runtime trace through the tiny stdio upstream fixture.
4. Add durable HTTP session storage and strict session lifecycle semantics.
5. Implement remote Streamable HTTP upstream connector with auth/token isolation and SSRF controls.
6. Add registry-backed discovery/import as a separate data source, not a hardcoded Rust catalog.
