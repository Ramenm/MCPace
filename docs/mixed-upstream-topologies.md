# Mixed upstream topology hardening

MCPace must keep working when a user configures many upstream MCP servers that do not share the same transport, reliability profile, tool count, or safety posture. The design target is not just "one stdio server works"; it is mixed fan-out across local commands, local/plain Streamable HTTP endpoints, blocked remote HTTPS endpoints, legacy HTTP+SSE endpoints, disabled/profile-gated servers, missing commands, bad working directories, slow servers, invalid JSON responses, and unsupported custom transports.

## Source-type matrix

| configured kind | normalized `sourceType` | direct forwarding | expected status | operational rule |
|---|---|---:|---|---|
| local stdio command | `stdio` | yes | `callable-stdio` | Launch as subprocess, isolate stderr, cache only `tools/list`, call through `upstream_call` / `upstream_batch`. |
| local/plain Streamable HTTP | `http` with `http://` URL | yes | `callable-http` | Use MCP Streamable HTTP POST flow. Session is runtime-only; do not persist it across restart. |
| HTTPS remote Streamable HTTP | `http` with `https://` URL | no | `blocked-https-upstream` | Direct TLS client is not implemented in this build; use a stdio bridge such as `mcp-remote` or a local gateway. |
| legacy HTTP+SSE | `legacy-sse` | no | `blocked-legacy-sse-upstream` | Do not silently treat old SSE endpoints as Streamable HTTP. Use an adapter or migrate endpoint. |
| custom transport | original token | no | `blocked-unsupported-transport` | Preserve as diagnostics, not a runtime panic. |
| disabled/profile/platform gated | normalized source | no | `disabled` | Show reason and exclude from live catalog fan-out. |
| missing command / bad cwd | `stdio` | no | `blocked-command-not-found` | Keep the server visible in inventory with repair guidance. |
| slow / invalid runtime | normalized source | no tools | `catalog-failed` | Failure must be isolated to that server; other servers continue. |

## Mixed-server invariants

1. **One server cannot poison the topology**. Timeouts, invalid JSON, missing commands, unsupported transports, and disabled entries return per-server diagnostics instead of aborting all discovery.
2. **Tool names are server-scoped**. Duplicate tool names across servers are expected; all cross-server views must include `server`, `name`, and `qualifiedName` or a projected hash name.
3. **Projection is bounded**. Large mixed catalogs must keep broker discovery available and expose only budgeted native projections.
4. **Search is bounded and server-aware**. `upstream_search` must scan nested server catalogs and retain bounded top-k results, not flatten every tool into an unbounded response.
5. **HTTP transport compatibility is explicit**. Plain local Streamable HTTP can be forwarded directly; HTTPS and legacy HTTP+SSE are blocked with exact diagnostics rather than being misclassified as generic HTTP.
6. **Stateful work stays per-server**. `upstream_batch` is same-server. Cross-server batches must be planned as multiple lanes because sessions, leases, process pools, and HTTP session IDs do not cross server boundaries.
7. **Cache keys include transport identity**. Cache fingerprints include source type, command/url, args, cwd, timeout, env hash, MCPace version, and MCP protocol version so reinstall/upgrade and transport changes do not reuse stale tool lists.

## Verification

Run:

```bash
npm run verify:mixed-upstreams
```

Default simulation:

- 50 configured servers.
- 200,000 configured tool slots.
- mixed stdio, HTTP, HTTPS, legacy SSE, disabled, missing-command, bad-cwd, unsupported, timeout, invalid-JSON servers.
- duplicate tool names across servers.
- bounded search top-k, projection budget, and first page size.
- failure isolation: some servers fail while successful stdio/plain HTTP servers still provide tools.

This is intentionally a source-level synthetic proof. It does not replace real-client traces or Rust-host compile proof.
