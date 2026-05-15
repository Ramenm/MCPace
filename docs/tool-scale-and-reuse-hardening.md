# Tool scale and reuse hardening

This document is the operating contract for large upstream catalogs: tens of MCP servers and
100k-200k+ aggregate tools. The goal is not to make every tool a top-level native tool; the goal is
to keep MCPace responsive, reusable, restart-safe, and predictable when the upstream ecosystem is
large.

## Scale target

Baseline stress target:

- 50 configured callable upstream servers.
- 100,000 to 200,000 aggregate upstream tools.
- `tools/list` from the MCPace endpoint must remain small by default.
- Search and routing must use bounded result sets.
- Native projection must be explicitly budgeted.
- Full-catalog dumps must be treated as diagnostics/export, not startup behavior.

## Default model

1. **Broker-first startup**: `MCPACE_TOOL_EXPOSURE=broker` remains the default. Startup `tools/list`
   exposes MCPace broker/adapter tools and does not fan out to every upstream.
2. **Search before projection**: use `upstream_search` for discovery across many servers. It returns
   compact ready-to-call results, not every schema.
3. **One-server full schema when needed**: use `upstream_tools(server)` for full schema inspection of
   one server.
4. **Native projection only by budget**: `auto`, `hybrid`, and `native` modes are count/token bounded.
5. **Explicit call path**: calls go through `upstream_call` or `upstream_batch`, preserving the same
   policy, known-tool validation, lease, and session-pool layer as projected tools.

## Response and memory budgets

| Surface | Large-catalog behavior |
|---|---|
| MCP `tools/list` | Cursor pagination is available via `MCPACE_TOOL_PAGE_SIZE`; broker mode avoids live fan-out. |
| `upstream_search` | Scans nested server listings and keeps only bounded top-k results. It must not build an all-tools flat catalog first. |
| `upstream_catalog` | All-server catalog response is bounded by `MCPACE_CATALOG_TOOL_LIMIT` and per-server samples by `MCPACE_CATALOG_SERVER_TOOL_SAMPLE_LIMIT`. Use `upstream_tools(server)` for full per-server schema. |
| Projection catalog | Stores bounded projection candidates via `MCPACE_PROJECTION_CANDIDATE_LIMIT` and bounded broker-only samples via `MCPACE_PROJECTION_BROKER_SAMPLE_LIMIT`. Counts still report the full scanned space. |
| Tool-list cache | Cache keys include settings fingerprint, server fingerprint, MCPace version, and MCP protocol version. Disk cache is disposable and version-sensitive. |

## Environment knobs

| Variable | Default | Purpose |
|---|---:|---|
| `MCPACE_TOOL_EXPOSURE` | `broker` | Avoids startup fan-out and huge native tool lists by default. |
| `MCPACE_TOOL_BUDGET` | `64` | Max projected top-level tools. |
| `MCPACE_TOOL_TOKEN_BUDGET` | `24000` | Approximate token budget for projected tools. |
| `MCPACE_PROJECTED_TOOL_SAFETY` | `safe` | Only clearly read-only tools are natively projected by default. |
| `MCPACE_ALLOW_UNKNOWN_UPSTREAM_TOOLS` | unset / false | Keep upstream calls fail-closed unless the tool name is advertised by `tools/list`. |
| `MCPACE_TOOL_PAGE_SIZE` | unset | Enables cursor paging for MCPace `tools/list`; clamped to 1..512. |
| `MCPACE_CATALOG_TOOL_LIMIT` | `2000` for all-server catalog | Caps all-server flat catalog output. Server-specific catalog stays full unless this is set. |
| `MCPACE_CATALOG_SERVER_TOOL_SAMPLE_LIMIT` | `200` | Caps tools retained inside each server object in all-server catalog responses. |
| `MCPACE_PROJECTION_CANDIDATE_LIMIT` | `MCPACE_TOOL_BUDGET * 8`, max 8192 | Caps retained native-projection candidates while still counting full catalog size. |
| `MCPACE_PROJECTION_BROKER_SAMPLE_LIMIT` | `64` | Caps retained broker-only sample metadata in projection diagnostics. |

## Reuse rules

- Reuse cached `tools/list` results unless `refresh=true` or `MCPACE_TOOLS_LIST_REFRESH=true` is set.
- Reuse upstream pooled sessions for same-server stateful sequences when policy allows; prefer
  `upstream_batch` for same-server multi-step work.
- Do not use the full all-server catalog as a routing cache in clients. Use `upstream_search` and the
  returned `call` object.
- If an upstream tool package is reinstalled and the command/config fingerprint does not change, use
  refresh or cleanup cache to force rediscovery.

## Verification

Run:

```bash
npm run verify:tool-scale
```

This executes a deterministic 50-server / 200k-tool simulation and checks that top-k search,
projection candidates, page size, and memory budget stay bounded. It is intentionally separate from
fast smoke tests because it is a scale gate, not a syntax gate.

## Failure policy

If the scale gate fails:

1. Keep `MCPACE_TOOL_EXPOSURE=broker` as the safe fallback.
2. Lower `MCPACE_TOOL_BUDGET` and set `MCPACE_TOOL_PAGE_SIZE`.
3. Use `upstream_tools(server)` instead of all-server `upstream_catalog`.
4. Clear only disposable caches; do not delete durable user config.
5. Treat native projection as disabled until the bounded simulation passes again.
