# MCPace runbook

MCPace is a local home for MCP servers: one endpoint for clients, safe concurrency for upstream servers, and explicit review before unknown packages run.

## Requirements

| Tool | Use |
|---|---|
| Rust/Cargo | Build and test the native `mcpace` binary. |
| Node.js 22+ and npm 10+ | Run Node checks and npm-based MCP servers. |
| `uvx` | Optional PyPI MCP server launcher. |
| Docker | Optional OCI/container MCP server launcher. |

## Documentation map

| File | Keep here | Do not duplicate here |
|---|---|---|
| `README.md` | Short landing page and first commands. | Full runbook details. |
| `docs/README.md` | Operator flow and doc navigation. | Deep classifier history. |
| `docs/architecture.md` | Scheduler model, modes, state classes. | CLI option reference. |
| `docs/configuration.md` | Files, config shapes, discovery settings, policy options. | Lab corpus details. |
| `docs/lab-harness.md` | Evidence corpus, random sweeps, safe probe boundary. | Basic install steps. |
| `reports/summary.md` | Release/source-bundle summary and validation status. | User manual content. |

## First run

```bash
cargo install --path .
mcpace up
```

On a new machine, `mcpace up` creates:

```text
~/.mcpace/mcpace.config.json
~/.mcpace/mcp_settings.json
~/.mcpace/mcp_settings.d/
```

It imports existing local MCP servers when safe, skips MCPace self-references, starts the local endpoint, patches only detected clients, and prints the next explicit command when no upstream servers are present.

## Accepted MCP config shapes

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
      "url": "http://127.0.0.1:8010/mcp"
    }
  }
}
```


Direct upstream forwarding currently supports stdio and plain/local Streamable HTTP. HTTPS remote endpoints should be connected through a stdio adapter such as `mcp-remote` or a local HTTP gateway until native TLS upstream forwarding is added.

Normalization rules are intentionally small: command entries become stdio servers; URL aliases become Streamable HTTP servers; `disabled: true` becomes `enabled: false`; MCPace's own endpoint is skipped to avoid loops.

## Common workflows

| Need | Command |
|---|---|
| Start/repair home | `mcpace up` |
| Avoid client patching | `mcpace up --client none` |
| Preview an install | `mcpace install npm:@modelcontextprotocol/server-memory --as memory --dry-run` |
| Add a local/plain HTTP gateway | `mcpace install http://127.0.0.1:8010/mcp --as local-gateway` |
| Import existing settings | `mcpace server import ./mcp.json --dry-run` |
| Discover trusted servers | `mcpace auto --dry-run` then `mcpace auto` |
| Inspect loaded sources | `mcpace server sources --json` |
| Inspect planned routing | `mcpace server instances --client-id cursor --session-id chat-a --project-root .` |
| Inspect active leases | `mcpace server leases --json` |
| Open the dashboard | `mcpace dashboard` |

## Concurrency policy workflow

```bash
mcpace server set-policy filesystem --mode session-isolated --affinity client,project,chat
mcpace server set-policy fetch --mode pool --max-workers 4 --queue-timeout-ms 5000
mcpace server list --json
```

Use `shared` for proven stateless servers, `serialized` for fragile shared state, `session-isolated` for chat/client state, `project-isolated` for repo/worktree state, `pool` for scalable stateless workers, and `disabled` for broken or unsafe servers.

## Dynamic discovery

```bash
mcpace auto --dry-run
mcpace auto
mcpace auto filesystem --json
```

Auto mode refreshes stale registry metadata, chooses approved or trusted candidates, writes reviewable server fragments, and probes `initialize` plus `tools/list` before relaxing runtime policy. Unknown public packages stay plan-only until local trust policy or explicit review allows them.

## Source verification

```bash
npm run lint:npm
npm run test:npm
npm run check
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
npm run load:local -- --binary ./target/release/mcpace --duration-ms 5000 --concurrency 64
```

`npm run check` covers Node syntax, npm launcher tests, docs/package hygiene, release-artifact dry runs, and static MCP import-normalization guards. Run Rust checks on a host with the pinned Rust toolchain.

## Archive policy

The release ZIP contains one root directory with source code, required configs, compact docs, examples, schemas, tests, evaluation fixtures, and the summary report.

Excluded by design: `.git`, `node_modules`, caches, temporary files, OS artifacts, runtime data/logs/backups, vendored platform binaries, and heavyweight build output.

## Safe probe for weak servers

```bash
mcpace lab probe --refresh --timeout-ms 30000
mcpace lab probe --id filesystem --refresh --json
```

The probe performs `initialize`, `notifications/initialized`, and `tools/list` only. It never calls upstream tools.
