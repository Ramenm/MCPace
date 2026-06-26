# MCPace

MCPace runs MCP servers at the right concurrency.

MCPace is a local MCP process scheduler for concurrent AI agents. It gives every client one local endpoint, then decides whether each upstream MCP server should be shared, serialized, isolated per chat/project, pooled, or disabled.

## Install

```bash
npm install -g @mcpace/cli@latest
mcpace up
```

Node.js 22+ is required. GitHub Releases also provide native `.msi`, `.deb`, and `.pkg` installers; see [`docs/release-completion.md`](docs/release-completion.md). Source installs for local development still work with `cargo install --path .`.

`mcpace up` creates or repairs `~/.mcpace`, imports safe existing MCP servers from detected local configs, starts `http://127.0.0.1:39022/mcp`, wires detected clients, and runs readiness checks. It does **not** add a filesystem server, memory server, or any other upstream server by default.

## Daily commands

| Need | Command |
|---|---|
| Start or repair the local home | `mcpace up` |
| Preview auto discovery | `mcpace auto --dry-run` |
| Add trusted discovered servers | `mcpace auto` |
| Add an explicit server | `mcpace install npm:@modelcontextprotocol/server-memory --as memory` |
| Import an existing config | `mcpace server import ./mcp.json --dry-run` |
| Set routing policy | `mcpace server set-policy filesystem --mode session-isolated --affinity client,project,chat` |
| Inspect routing | `mcpace server instances --client-id cursor --session-id chat-a --project-root .` |
| Watch the local UI | `mcpace dashboard` |

## Runtime policy

Modes are `shared`, `serialized`, `session-isolated`, `project-isolated`, `pool`, and `disabled`. MCPace starts conservatively: unproven stdio servers stay serialized or isolated until metadata, policy, or a safe `initialize`/`tools/list` probe proves a wider mode is safe.

## Documentation

| File | Purpose |
|---|---|
| [`docs/README.md`](docs/README.md) | Runbook and documentation map. |
| [`docs/architecture.md`](docs/architecture.md) | Scheduler model, planes, modes, and state classes. |
| [`docs/dashboard-base.md`](docs/dashboard-base.md) | Dashboard information architecture, display order, and action rules. |
| [`docs/configuration.md`](docs/configuration.md) | Config files, install/import examples, dynamic discovery, and policy options. |
| [`docs/lab-harness.md`](docs/lab-harness.md) | Evidence corpus for automatic runtime classification. |
| [`SECURITY.md`](SECURITY.md) | Vulnerability reporting and security boundary. |

## License

MCPace is licensed under Apache-2.0. Copyright 2026 Ramenm.

## Verify

```bash
npm run check
npm run check:rust
cargo build --release
npm run load:local -- --binary ./target/release/mcpace --duration-ms 5000 --concurrency 64
```

This ZIP is source-only: no `.git`, `node_modules`, caches, runtime logs, vendored binaries, Rust `target`, or heavy build outputs.
