# MCPace universal dynamic adapter

MCPace is intended to be a gateway/adaptor between many MCP clients and many configured upstream MCP servers. The runtime does **not** depend on a hardcoded brand map of clients or servers. The runtime source of truth is:

1. the client's MCP `initialize` protocol version and capabilities,
2. the active transport/context,
3. configured upstream server inventory,
4. live upstream `tools/list`, `prompts/list`, and `resources/list` responses,
5. MCPace policy, leases, conflict domains, and session-pool state.

Static client/server catalogs may still exist for install/export helpers, but they are not runtime authority.

## Tool exposure modes

`MCPACE_TOOL_EXPOSURE` controls how upstream tools are surfaced:

- `broker` (default): MCPace keeps startup `tools/list` cheap and token-small by exposing broker/search/call tools without probing every configured upstream.
- `auto`: MCPace probes the live upstream catalog and projects upstream tools as native top-level tools only when the whole projectable catalog fits both the count and estimated-token budgets. Otherwise MCPace keeps a compact broker/search surface.
- `hybrid`: project the highest-ranked prefix that fits the budgets and keep broker/search for the rest.
- `native`: project upstream tools as top-level tools up to the configured budgets.
- `minimal`: expose only the essential adapter tools for strict clients.

Useful related settings:

```bash
MCPACE_TOOL_EXPOSURE=broker
MCPACE_TOOL_BUDGET=64
MCPACE_TOOL_TOKEN_BUDGET=24000
MCPACE_TOOLS_LIST_TIMEOUT_MS=5000
MCPACE_TOOLS_LIST_REFRESH=false
MCPACE_TOOL_PAGE_SIZE=50
MCPACE_TOOL_SCHEMA_STYLE=native
```

## Native projection

Projected upstream tools use names like:

```text
u_<server>_<tool>_<hash>
```

The name is generated from short MCP-client-safe slugs plus a stable hash, while the original upstream server/tool pair is stored in `_meta["mcpace/upstream"]`. Slash namespacing, dots, and double-underscore separators are intentionally avoided because real clients can be stricter than the MCP spec.

Projection now uses the full live upstream `tools/list` definition instead of a hand-written summary. MCPace preserves the upstream tool's normal MCP fields, rewrites only the projected `name`, prefixes the description with routing context, and adds MCPace routing metadata. JSON schemas are compacted by default to reduce tool-list tokens while preserving required fields and structure; set this to keep schemas raw:

```bash
MCPACE_PROJECTED_SCHEMA_COMPACTION=false
```

## Projection safety

`MCPACE_PROJECTED_TOOL_SAFETY` controls which upstream tools are allowed to become native projected tools:

- `safe` (default): the projection safety default is `safe`; project only tools that look read-only via trusted annotations or conservative names.
- `review`: project tools that are not guarded by MCPace policy and do not look mutating/destructive.
- `all`: project everything that fits the budget and rely on the client/human-in-the-loop for review.

Tools that do not pass projection safety are still callable through `upstream_search` and `upstream_call`, where explicit policy controls remain visible.

## Projected tool adapter controls

Projected tools are intended to behave like native upstream tools. MCPace therefore does **not** strip top-level upstream arguments such as `timeoutMs`, `metadata`, or `resultMode`, because a real upstream tool may legitimately define parameters with those names.

Use nested `_mcpace` or `mcpace` only for adapter controls:

```json
{
  "query": "issues",
  "_mcpace": {
    "timeoutMs": 10000,
    "resultMode": "native",
    "diagnostics": "none",
    "allowToolRiskClasses": true
  }
}
```

Legacy top-level adapter controls can be re-enabled only for old local workflows:

```bash
MCPACE_PROJECTED_LEGACY_TOP_LEVEL_CONTROLS=true
```

## Broker fallback

When projected tools are unavailable, unsafe, or the catalog is too large, the model can still use:

```text
upstream_search  -> concise live discovery
upstream_call    -> call one upstream tool
upstream_batch   -> call several tools on one upstream server in a state-preserving session
adapter_route    -> plan batching/parallelism for a set of intended upstream calls
```

This gives a native path for small/safe catalogs and a predictable low-token path for large catalogs.

## Dynamic prompts and resources

MCPace advertises prompts/resources capabilities and proxies upstream methods dynamically:

```text
prompts/list
prompts/get
resources/list
resources/templates/list
resources/read
```

Prompt names are projected as `p_<server>_<prompt>_<hash>`. Resource URIs are proxied through `mcpace://upstream-resource/<serverHex>/<uriHex>` so the client can read them through MCPace while MCPace preserves the original upstream server routing.

## Client adaptation

MCPace does not need a client-brand map to decide its main behavior. It uses protocol/capability negotiation:

- recent protocols get `title` and `annotations` in tool definitions;
- older protocol versions can receive a slimmer legacy schema;
- all clients can call `adapter_profile` to see the current routing and projection plan.

`MCPACE_TOOL_SCHEMA_STYLE=legacy|native` can override this when a client is known to be strict.

## Route planning and parallelism

There is no universal MCP guarantee that every server/tool is parallel-safe. MCPace therefore treats concurrency as runtime policy rather than a static server map:

- same-server stateful sequences use `upstream_batch`;
- independent single calls use `upstream_call` or projected native tools;
- `adapter_route` groups intended calls by server and conflict domain;
- leases and pooled sessions preserve per-client/per-session context;
- future plugins can add stronger per-server parallelism policies from observed tool annotations, server config, or explicit user rules.

## Plugin path

The current API already has token-reducer hooks. Future external plugins should fit into these categories:

- `tool-catalog-ranker`: rank or hide low-value upstream tools before native/hybrid projection;
- `schema-compactor`: simplify verbose JSON schemas while preserving required fields;
- `resource-link-store`: move large outputs to temporary MCP resources;
- `client-capability-detector`: refine behavior from protocol, headers, initialize params, and transport quirks;
- `upstream-parallelism-policy`: learn or enforce which upstream servers/tools can run concurrently;
- `risk-classifier`: classify newly discovered tools when upstream annotations are missing.

The rule of thumb is: plugins may reduce, rank, annotate, or route what MCPace exposes, but should not invent server/tool capabilities that were not discovered from configuration or live MCP responses.
