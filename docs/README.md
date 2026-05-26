# MCPace runbook

MCPace is packaged as a small local home for MCP: start one endpoint, reuse servers that already exist in local MCP configs, and add new upstream servers only when the user explicitly asks for them. The sharper positioning is runtime adaptation: MCPace runs each upstream server at the safest useful concurrency.

## Requirements

- Rust/Cargo to build the native binary from this source bundle.
- Node.js 22+ and npm 10+ for npm-based MCP servers and Node-side checks.
- Optional: `uvx` for PyPI MCP servers, Docker for OCI/container MCP servers.

## One-command home setup

```bash
cargo install --path .
mcpace up
```

When MCPace has no root yet, `mcpace up` uses `~/.mcpace` and creates:

```text
~/.mcpace/mcpace.config.json
~/.mcpace/mcp_settings.json
~/.mcpace/mcp_settings.d/
```

The command does not invent a default upstream server. On an empty MCPace home it first scans existing local MCP configs, writes reusable entries to `mcp_settings.d/auto-imported-home.json`, skips MCPace self-references, starts the endpoint, patches only detected clients, and verifies readiness. If no existing servers are found, it starts clean and prints the next explicit command.

## Accepted config shapes

MCPace accepts both common MCP shapes:

```json
{
  "mcpServers": {
    "memory": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-memory"]
    }
  }
}
```

```json
{
  "servers": {
    "remote": {
      "url": "https://example.com/mcp"
    }
  }
}
```

Import normalization is intentionally small and predictable:

- `command` -> `type: "stdio"`
- `url`, `serverUrl`, `httpUrl`, or `endpoint` -> `url` + `type: "streamable-http"`
- `transport` aliases such as `http`, `remote`, `command`, or `stdio` are normalized
- `disabled: true` -> `enabled: false`
- MCPace's own endpoint or launcher entry is skipped to avoid loops

## Client behavior

By default, `mcpace up` uses `--client auto`:

```bash
mcpace up --client auto
mcpace up --client cursor-local
mcpace up --client all
mcpace up --client none
```

`auto` patches a client only when MCPace detects an existing local config or installed CLI. It preserves unrelated server entries and leaves configs alone when no supported client is detected.

## Concurrency policy workflow

```bash
mcpace server set-policy filesystem --mode session-isolated --affinity client,project,chat
mcpace server set-policy fetch --mode pool --max-workers 4 --queue-timeout-ms 5000
mcpace server instances --client-id cursor --session-id chat-a --project-root .
mcpace server leases --json
```

Use `shared` for stateless servers, `serialized` for fragile shared state, `session-isolated` for chat/client state, `project-isolated` for repo/worktree state, and `pool` for scalable stateless workers.

`mcpace server list --json` and the dashboard now expose `runtimeType`, `stateClass`, and `effectClass`, so users see whether a server was treated as stateless, session-stateful, project-stateful, credential/remote stateful, host-interactive, process-side-effecting, legacy, or unknown-conservative.

## Add servers without choosing a type

```bash
mcpace install npm:@modelcontextprotocol/server-memory --as memory
mcpace install pypi:mcp-server-demo --as demo
mcpace install oci:ghcr.io/example/mcp-server --as container-demo
mcpace install https://example.com/mcp --as remote
mcpace install . --as filesystem
mcpace install -- npx -y @modelcontextprotocol/server-memory
```

Use `--dry-run` before writing and `--as <name>` to choose the server name. Use `mcpace server sources` to see exactly what MCPace loaded.

## Discover servers dynamically

```bash
mcpace auto --dry-run
mcpace auto
mcpace auto filesystem --json
```

`mcpace auto` is the one-command dynamic path. It refreshes the registry cache only when needed, normalizes registry/catalog package metadata into the same install planner used by `mcpace install`, writes approved/trusted server fragments, and probes live `tools/list` so MCPace can classify the runtime surface without asking the user to choose a server type. Advanced `mcpace server discover --refresh`, `--auto-install`, and `--allow-review` remain available for debugging and team policy work.

Use this workflow for unknown servers:

1. `mcpace auto <query> --dry-run` to see the candidate and install/probe plan.
2. Review the package source, env/header needs, trust level, and projected policy in the JSON output.
3. Add the server to `catalog/approved-servers.json` or run an explicit `mcpace install ...` only after review.
4. Run `mcpace auto <query>` again; approved/trusted candidates are written and probed automatically.
5. Use `mcpace server test <name> --refresh --json` only when you want to force a new live `initialize`/`tools/list` probe.

## Lab evidence for auto mode

```bash
mcpace lab
mcpace lab coverage
mcpace lab show --id popular-npm-filesystem
```

The lab harness is the maintainer proof surface for auto mode. It uses fixtures from `eval/fixtures/runtime` plus `eval/popular-server-corpus.json` to show `server -> evidence -> runtimeType/stateClass/effectClass -> concurrencyPolicy`. See [`lab-harness.md`](lab-harness.md).

## Useful checks

```bash
mcpace serve status
mcpace doctor --json
mcpace server list --json
mcpace server sources --json
mcpace server test <name> --refresh --json
mcpace dashboard
```

The dashboard is the UI surface for day-to-day operations: it shows server posture, policy review, planned instances, leases, logs, and tool-call audit events.

## Source verification

```bash
npm run lint:npm
npm run test:npm
npm run check
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release
npm run load:local -- --binary ./target/release/mcpace --duration-ms 5000 --concurrency 64
```

`npm run check` covers Node syntax, npm launcher tests, docs/package hygiene, and static MCP import-normalization guards. Run the Rust checks on a host with the Rust toolchain installed.

`npm run load:local` starts a release or debug MCPace binary against an isolated temporary root, keeps upstream warmup disabled, exercises `/healthz`, `/api/overview`, and `/mcp`, and verifies important HTTP/MCP edge cases such as spoofed Host, cross-origin POST, missing Streamable HTTP `Accept`, oversized body, and unknown session id rejection.

## Archive policy

The release ZIP contains one root directory with source code, needed configs, compact docs, examples, schemas, tests, and reports.

Excluded by design: `.git`, `node_modules`, caches, temporary files, OS artifacts, runtime data/logs/backups, vendored platform binaries, and heavyweight build output.


### Safe probe for weak servers

When `mcpace auto` cannot prove a server from metadata, use the one-step lab probe. It performs `initialize + notifications/initialized + tools/list` only and never calls upstream tools:

```bash
mcpace lab probe --refresh --timeout-ms 30000
```
