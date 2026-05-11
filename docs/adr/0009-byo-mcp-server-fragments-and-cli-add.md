# ADR 0009 — BYO MCP server fragments and native `server add`

## Context

MCPace intentionally ships with no upstream MCP servers enabled by default. Users need a native way to add their own MCP servers without editing a single root JSON file by hand or rebuilding MCPace.

The MCP model lets servers expose tools/resources/prompts, and those tools are discovered by name and schema at runtime. MCPace should therefore optimize for a flexible server registry and client-facing brokering rather than a hardcoded upstream catalog.

## Problem

Before this pass, users could add upstream servers by editing `mcp_settings.json`, configured include files, or environment-provided settings files. That worked, but it was not convenient enough for repeated onboarding, testing, or per-project server composition.

The missing product affordances were:

- a command to inspect the exact MCP settings sources seen by runtime;
- a command to add one upstream server without manual JSON editing;
- a default fragment directory that can grow one file per server;
- source contracts that keep runtime, doctor/readiness, server inventory, and archive packaging aligned.

## Decision

Add:

```bash
mcpace server sources --json
mcpace server add <name> --command <cmd> [--arg <arg>...] [--env KEY=VALUE...] [--dry-run] [--force]
mcpace server add <name> --url <url> [--type http|streamable-http] [--header KEY=VALUE...] [--dry-run] [--force]
```

`server add` writes `mcp_settings.d/<normalized-name>.json` by default. The directory is part of the release manifest and is listed in `mcpace.config.json` through `mcpSettings.includeDirs`.

The registry load order is:

1. root `mcp_settings.json`;
2. default `mcp_settings.d/*.json`;
3. configured include files/directories;
4. `MCPACE_MCP_SETTINGS` files;
5. `MCPACE_MCP_SETTINGS_DIRS` directories.

Later duplicate normalized server names override earlier entries and are reported as warnings.

## Consequences

Positive:

- less hardcoded/manual onboarding;
- one-server-per-file composition;
- easier project-local server experiments;
- safer dry-run/force workflow;
- a native source inventory command for troubleshooting.

Tradeoffs:

- root JSON and fragments can now both define the same normalized server name; override warnings must stay visible;
- remote HTTP upstream entries can be inventoried, but callable remote HTTP fan-out remains future work;
- Rust compile/test still has to be verified on a machine with the pinned toolchain.

## Verification

Source-level verification:

```bash
npm test
node scripts/audit-source.mjs --json
node scripts/verify-npm-pack.mjs --json
node scripts/build-release-artifacts.mjs --json
```

Rust/runtime verification still required:

```bash
cargo fmt --all -- --check
cargo test --all-targets --locked
cargo build --release --locked
```
