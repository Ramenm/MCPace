# MCPace runbook

MCPace is packaged as a small local home for MCP: start one endpoint, reuse servers that already exist in local MCP configs, and add new upstream servers only when the user explicitly asks for them.

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

## Useful checks

```bash
mcpace serve status
mcpace doctor --json
mcpace server list --json
mcpace server sources --json
mcpace server test <name> --refresh --json
```

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

The release ZIP contains one root directory with source code, needed configs, compact docs, examples, schemas, tests, and `reports/summary.md`.

Excluded by design: `.git`, `node_modules`, caches, temporary files, OS artifacts, runtime data/logs/backups, vendored platform binaries, and heavyweight build output.
