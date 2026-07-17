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

## Commands

The human CLI is intentionally small. Generated client configs still use hidden `stdio` entrypoints, and installed login items still use the hidden `agent` entrypoint; those are transport/runtime contracts, not interactive commands.

| Need | Command |
| --- | --- |
| Install, repair, start, and configure login startup | `mcpace up` |
| Start an existing configuration | `mcpace start` |
| Stop now but keep startup enabled for the next login | `mcpace stop` |
| Restart without changing clients or startup | `mcpace restart` |
| Read aggregate runtime/startup status | `mcpace status --json` |
| Add an explicit server | `mcpace install npm:@modelcontextprotocol/server-memory --as memory` |
| Discover or inspect servers | `mcpace advanced server auto --dry-run` |
| Import an existing config | `mcpace advanced server import ./mcp.json --dry-run` |
| Manage client patches | `mcpace advanced client list` |
| Diagnose readiness | `mcpace advanced doctor --json` |
| Prove the registered login path without rebooting | `mcpace advanced autostart prove --json` |
| Run the local UI in the foreground | `mcpace advanced runtime foreground` |
| Preview complete local integration removal | `mcpace uninstall --dry-run` |

Run `mcpace advanced --help` for server, client, startup, runtime, lease, update, and maintainer groups. Removed pre-cleanup aliases fail explicitly instead of silently changing meaning; see [`docs/cli-migration.md`](docs/cli-migration.md) for replacements.

Before removing the npm package, run `mcpace uninstall --dry-run`, then `mcpace uninstall`, and only then `npm uninstall -g @mcpace/cli`. npm does not provide a reliable cross-platform hook for removing per-user Windows Run/systemd/LaunchAgent state.

Modes are `shared`, `serialized`, `session-isolated`, `project-isolated`, `pool`, and `disabled`. MCPace starts conservatively: unproven stdio servers stay serialized or isolated until metadata, policy, or a safe `initialize`/`tools/list` probe proves a wider mode is safe.

## Documentation

| File | Purpose |
| --- | --- |
| [`docs/README.md`](docs/README.md) | Runbook and documentation map. |
| [`docs/architecture.md`](docs/architecture.md) | Scheduler model, planes, modes, and state classes. |
| [`docs/cli-migration.md`](docs/cli-migration.md) | Canonical replacements for removed commands and flags. |
| [`docs/dashboard-base.md`](docs/dashboard-base.md) | Dashboard information architecture, display order, and action rules. |
| [`docs/configuration.md`](docs/configuration.md) | Config files, install/import examples, dynamic discovery, and policy options. |
| [`docs/lab-harness.md`](docs/lab-harness.md) | Evidence corpus for automatic runtime classification. |
| [`SECURITY.md`](SECURITY.md) | Vulnerability reporting and security boundary. |

## License

MCPace is licensed under Apache-2.0. Copyright 2026 Ramenm.
