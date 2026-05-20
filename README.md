# MCPace

One MCP endpoint for all your AI clients.

MCPace is a local MCP hub. It gives Cursor, Claude Code, VS Code, Windsurf, Codex, Gemini CLI, Kiro, and other MCP-compatible clients one stable local URL while keeping each client route isolated.

```text
http://127.0.0.1:39022/mcp
```

## Install from this source bundle

```bash
cargo install --path .
mcpace up
```

`mcpace up` is home-first: it creates or repairs `~/.mcpace`, imports existing MCP servers from detected local configs, starts the local endpoint, wires detected clients, and runs readiness checks. It does **not** add a filesystem server, memory server, or any other upstream server by default.

## Add or import servers only when you choose

```bash
mcpace install npm:@modelcontextprotocol/server-memory --as memory
mcpace install https://example.com/mcp --as remote
mcpace install . --as filesystem
mcpace server import ./mcp.json --dry-run
```

Server type is inferred from the input: `command` means stdio, URL fields mean remote HTTP, and explicit local paths map to the filesystem server.

## Why it helps

- One local MCP URL instead of per-client wiring.
- Existing `mcpServers` and VS Code-style `servers` configs can be reused.
- Each AI client gets its own isolated upstream session, so clients do not step on each other.

## Verify

```bash
npm run check
cargo test
```

Read the runbook in [`docs/README.md`](docs/README.md). This ZIP is source-only: no `.git`, `node_modules`, caches, runtime logs, vendored binaries, or heavy build outputs.
