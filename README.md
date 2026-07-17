# MCPace

MCPace runs MCP servers at the right concurrency.

MCPace is a local MCP process scheduler for concurrent AI agents. It gives every client one local endpoint, then decides whether each upstream MCP server should be shared, serialized, isolated per chat/project, pooled, or disabled.

## Install

```bash
npm install -g @mcpace/cli@latest
mcpace up
```

Node.js 22+ is required. This is the supported public install path: npm automatically selects the matching internal native package for Windows, glibc Linux, or macOS; do not install `@mcpace/cli-<target>` packages directly. Native `.msi`, `.deb`, and `.pkg` artifacts are currently private draft proofs, not public downloads; Windows and macOS publication remains blocked on signing and notarization. See [`docs/release-completion.md`](docs/release-completion.md). Source installs for local development still work with `cargo install --path .`.

`mcpace up` creates or repairs `~/.mcpace`, imports safe existing MCP servers from detected local configs, starts `http://127.0.0.1:39022/mcp`, wires detected clients, installs or repairs user-level autostart, and runs readiness checks. The first managed runtime is immediately owned by the current user's supervisor: the hidden MCPace launcher on Windows, `systemd --user` on Linux, or a LaunchAgent on macOS. The same supervisor restores the endpoint after the next login and restarts failed runtimes. Use `mcpace up --no-autostart` for a session-only runtime. On WSL, the distribution must use systemd and Windows must start the distribution before its Linux user service can run. MCPace does **not** add a filesystem server, memory server, or any other upstream server by default.

## Daily commands

| Need | Command |
| --- | --- |
| Start or repair the local home and autostart | `mcpace up` |
| Run without future login startup | `mcpace up --no-autostart` |
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
| --- | --- |
| [`docs/README.md`](docs/README.md) | Runbook and documentation map. |
| [`docs/architecture.md`](docs/architecture.md) | Scheduler model, planes, modes, and state classes. |
| [`docs/dashboard-base.md`](docs/dashboard-base.md) | Dashboard information architecture, display order, and action rules. |
| [`docs/configuration.md`](docs/configuration.md) | Config files, install/import examples, dynamic discovery, and policy options. |
| [`docs/lab-harness.md`](docs/lab-harness.md) | Evidence corpus for automatic runtime classification. |
| [`SECURITY.md`](SECURITY.md) | Vulnerability reporting and security boundary. |

## License

MCPace is licensed under Apache-2.0. Copyright 2026 Ramenm.

## Contributor verification

The commands below are a fast local contributor check, not release approval. Release hosts must run the locked, fail-closed sequence in [`docs/release-readiness.md`](docs/release-readiness.md).

```bash
npm run check
npm run check:rust
cargo build --release --locked
npm run load:local -- --duration-ms 5000 --concurrency 64
```
