# MCPace runbook

MCPace is a local home for MCP servers: one endpoint for clients, safe concurrency for upstream servers, and explicit review before unknown packages run.

## Requirements

| Tool | Use |
| --- | --- |
| Rust/Cargo | Build and test the native `mcpace` binary. |
| Node.js 22+ and npm 10+ | Run Node checks and npm-based MCP servers. |
| `uvx` | Optional PyPI MCP server launcher. |
| Docker | Optional OCI/container MCP server launcher. |

## Documentation map

- [dashboard-base.md](dashboard-base.md) — dashboard information architecture and base setup rules.
- [frontend.md](frontend.md) — dashboard frontend shell/assets, rendering ownership, and accessibility rules.

| File | Keep here | Do not duplicate here |
| --- | --- | --- |
| `README.md` | Short landing page and first commands. | Full runbook details. |
| `docs/README.md` | Operator flow and doc navigation. | Deep classifier history. |
| `docs/architecture.md` | Scheduler model, modes, state classes. | CLI option reference. |
| `docs/configuration.md` | Files, config shapes, discovery settings, policy options. | Lab corpus details. |
| `docs/frontend.md` | Dashboard frontend assets, ownership, and first-screen contract. | Full backend overview schema details. |
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

Direct upstream forwarding supports stdio and Streamable HTTP. Remote HTTPS endpoints use platform certificate verification and configured authentication headers; redirects are disabled to prevent credential forwarding. Plain HTTP upstreams are restricted to exact loopback hosts.

Normalization rules are intentionally small: command entries become stdio servers; URL aliases become Streamable HTTP servers; `disabled: true` becomes `enabled: false`; MCPace's own endpoint is skipped to avoid loops.

## Common workflows

| Need | Command |
| --- | --- |
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

No-query auto mode uses the pinned embedded/local curated catalog; named searches refresh a bounded query-specific Registry cache. It writes reviewable server fragments and requires a successful, bounded `initialize` plus paginated `tools/list` probe before reporting readiness. Unknown/deprecated public packages, unsupported package managers, custom registry bases, missing required configuration, and malformed responses stay plan-only or fail closed.

## Source verification

```bash
npm run lint:npm
npm run test:npm
npm run check
npm run check:rust-boundaries
npm run check:rust
npm run check:ci
cargo build --release --locked --bins
npm run load:local -- --duration-ms 5000 --concurrency 64
```

`npm run check` is the quick local source gate. `npm run check:ci` is the fail-closed release-facing entrypoint; its endgame step owns the single live Rust check/test/format/Clippy proof. Run it on a host with the pinned Rust toolchain.

## Archive policy

The release ZIP contains one root directory with source code, required configs, compact docs, examples, schemas, tests, evaluation fixtures, and the summary report.

Excluded by design: `.git`, `node_modules`, caches, temporary files, OS artifacts, runtime data/logs/backups, vendored platform binaries, and heavyweight build output.

## Safe probe for weak servers

```bash
mcpace lab probe --refresh --timeout-ms 30000
mcpace lab probe --id filesystem --refresh --json
```

The probe performs `initialize`, `notifications/initialized`, and `tools/list` only. It never calls upstream tools.

## Dashboard base model

The first screen should answer five questions before exposing advanced controls: is the backend reachable, which client is wired, which source is saved, whether tools have been tested, and whether routing is still conservative. Import, discovery, and manual add forms should validate next to the affected field without clearing user input.

## Final source bundle

Source bundles use the generated name `mcpace-v<version>-<build-id>`. Read the adjacent artifact manifest for the exact name and hashes. Use the root `README.md` for the short start path, this runbook for operational details, `docs/frontend.md` for dashboard frontend rules, and `reports/summary.md` for the packaging and validation summary.

- [MCP transport contract](mcp-transport-contract.md)
- [Release readiness gate](release-readiness.md)

- [Rust live proof](rust-live-proof.md)
- [Endgame readiness](endgame-readiness.md)
- [Rust boundary contract](rust-boundary-contract.md)
