# MCPace

MCPace runs MCP servers at the right concurrency.

Share safe servers, serialize fragile ones, and clone stateful stdio servers per client, project, or chat — automatically.

MCPace is a local MCP process scheduler for concurrent AI agents. It keeps the simple `http://127.0.0.1:39022/mcp` workflow, but the real job is runtime adaptation: turn fragile single-session servers into reliable multi-session infrastructure.

## Install from this source bundle

```bash
cargo install --path .
mcpace up
```

`mcpace up` is home-first: it creates or repairs `~/.mcpace`, imports existing MCP servers from detected local configs, starts the local endpoint, wires detected clients, and runs readiness checks. It does **not** add a filesystem server, memory server, or any other upstream server by default.

## Set a server concurrency policy

```bash
mcpace server set-policy filesystem --mode session-isolated --affinity client,project,chat
mcpace server instances --client-id cursor --session-id chat-a --project-root .
mcpace server leases --json
```

Modes: `shared`, `serialized`, `session-isolated`, `project-isolated`, `pool`, and `disabled`.

## Auto-discover new MCP servers

```bash
mcpace auto --dry-run
mcpace auto
mcpace auto filesystem --json
```

`mcpace auto` is the normal path: refresh the registry cache when it is missing or stale, pick approved/trusted candidates, write server config, launch the package manager only during probe, read live `initialize`/`tools/list` evidence, and let MCPace infer safe runtime policy. Advanced `server discover --refresh/--auto-install/--allow-review` flags still exist for debugging, but users should not need to choose a server type.

Runtime type is also inferred automatically: stateless, session/project/credential stateful, external, host-interactive, side-effecting, legacy, or conservative unknown. Details live in [`docs/architecture.md`](docs/architecture.md).

## Add or import servers only when you choose

```bash
mcpace install npm:@modelcontextprotocol/server-memory --as memory
mcpace install https://example.com/mcp --as remote
mcpace server import ./mcp.json --dry-run
```

Server type is inferred from the input: `command` means stdio, URL fields mean remote HTTP, and explicit local paths map to the filesystem server.

## Watch what is happening

```bash
mcpace dashboard
```

The local dashboard shows health, server posture, policy review, planned concurrency instances, runtime leases, and a tool-call audit trail.

## Verify

Run `npm run check` and `cargo test`. Read the runbook in [`docs/README.md`](docs/README.md). This ZIP is source-only: no `.git`, `node_modules`, caches, runtime logs, vendored binaries, or heavy build outputs.
