# Dynamic native adapter surface

MCPace is designed to be one adapter that a user can connect to many MCP clients while keeping the user's real upstream MCP servers behind a single, stable endpoint.

The runtime does not depend on static client/server maps as authority. Static catalogs can still be useful for install/export helpers, but the live routing path is capability based:

1. Read the current MCP `initialize` payload: protocol version, `clientInfo`, capabilities, and `_meta` hints.
2. Shape `tools/list` for that protocol revision.
3. Keep startup `tools/list` cheap by default: expose broker tools without probing every upstream.
4. Discover configured upstream servers through live MCP `tools/list` calls only when a user/tool asks for live search/catalog data or when native projection is explicitly enabled.
5. Choose the projected top-level surface from the tool budget and live catalog size when projection is enabled.
6. Route projected tools, `upstream_call`, and `upstream_batch` through the same policy/lease/session layer.
7. Apply upstream coordination through the existing runtime policy, lease, and pooled-session layer instead of a client/server brand map. Stateful same-server sequences should use `upstream_batch`; single independent calls should use `upstream_call`.

## BYO MCP configuration model

MCPace uses a **Bring Your Own MCP servers (BYO MCP)** model. The packaged
distribution intentionally does not enable, recommend, or install upstream MCP
servers. The user's `mcp_settings.json.mcpServers` is the source of truth for
which upstreams exist. `mcpace.config.json.servers` is only an optional policy
overlay for routing, concurrency, platform gates, required commands, and tool
risk gates.

Adding a new upstream server should not require changing Rust code or rebuilding
MCPace. Once the user's command is installed and enabled in `mcp_settings.json`,
it becomes visible to inventory/probe/catalog/audit/call flows. HTTP-like
entries are inventoried honestly; stdio entries are the current callable bridge.

## Tool surface modes

`MCPACE_TOOL_EXPOSURE` controls how much of the upstream catalog is exposed directly to the client:

- `broker` (default): never project upstream tools during startup `tools/list`; use `upstream_search`, `upstream_catalog`, `upstream_call`, and `upstream_batch`.
- `auto`: project upstream tools as native top-level tools only when the projectable live catalog fits both the count and estimated-token budgets. This probes callable upstreams during `tools/list`, so use it only when the client needs native projected tools.
- `hybrid`: project the highest-ranked prefix that fits the budgets and keep `upstream_search`/`upstream_call` for the rest.
- `native`: project upstream tools directly, truncated to the configured budgets if needed.
- `minimal`: smallest surface for strict clients; keeps only the essential broker/adapter tools.

Related knobs:

- `MCPACE_MANAGEMENT_SURFACE=adapter` (default) keeps runtime adapter tools visible and keeps install/debug/client-catalog helpers out of normal `tools/list`. Use `minimal` for only `adapter_profile`, `upstream_search`, `upstream_call`, and `upstream_batch`. Full debugging/ops surfacing is privileged and requires both `MCPACE_MANAGEMENT_SURFACE=full` and `MCPACE_ALLOW_FULL_MANAGEMENT=1`.
- `MCPACE_TOOL_BUDGET=64` limits the projected native tools count budget.
- `MCPACE_TOOL_TOKEN_BUDGET=24000` limits the approximate projected tool-list token budget.
- `MCPACE_NATIVE_TOOL_BUDGET` and `MCPACE_NATIVE_TOOL_TOKEN_BUDGET` are accepted as aliases.
- `MCPACE_PROJECTED_TOOL_SAFETY=review|safe|all` controls which tools may be projected natively. The projection safety default is `safe`: only tools that look read-only via trusted annotations or conservative names may be projected. Use `review` or `all` only when a client explicitly needs broader native projection.
- `MCPACE_TOOLS_LIST_TIMEOUT_MS=5000` controls live catalog discovery during `tools/list`.
- `MCPACE_TOOLS_LIST_REFRESH=true` bypasses the short successful tools/list cache.
- `MCPACE_PAGE_SIZE=<n>` enables MCP cursor pagination for every dynamic list method.
- `MCPACE_TOOL_PAGE_SIZE`, `MCPACE_PROMPT_PAGE_SIZE`, `MCPACE_RESOURCE_PAGE_SIZE`, and `MCPACE_RESOURCE_TEMPLATE_PAGE_SIZE` override the generic page size per list method.

## Projected tool names

Projected upstream tools use generated names such as:

```text
u_<server>_<tool>_<hash>
```

The projected name uses a short MCP-client-safe slug plus a stable hash of the upstream server and tool names, then resolves against the same live catalog and reserved MCPace tool names. This avoids static maps, preserves original upstream names for calls, and prevents collisions with MCPace's own tools. Direct projected calls and brokered `upstream_call` both go through the same upstream policy, known-tool validation, and lease handling.

Projected tools pass normal upstream arguments through unchanged. Adapter-only controls such as `timeoutMs`, `resultMode`, `diagnostics`, and allow-flags belong inside a nested `_mcpace` or `mcpace` object so they cannot collide with a real upstream parameter.

## Runtime tools

### `adapter_profile`

Returns the negotiated dynamic adapter profile: client capabilities, advertised server capabilities, current tool-exposure mode, projection counts, concurrency model, plugin hooks, and upstream inventory. Use `includeLiveCatalog=true` only when a live catalog sample is needed.

### `adapter_route`

Builds a routing plan for intended upstream calls. It groups calls by upstream server and conflict domain, recommends `upstream_batch` when calls should share one upstream session, and tells a multi-tool-capable client which lanes can be run in parallel. The plan is generated from MCPace server policy plus optional live discovery, not from hardcoded client/server maps.

### `upstream_search`

Searches live configured upstream MCP tools and returns compact ready-to-call results. This avoids loading every upstream schema into the model context. Use `upstream_tools` only when the full schema for one server is needed.

### `upstream_call` and `upstream_batch`

`upstream_call` is the canonical single-call fallback. `upstream_batch` is the state-preserving path for multiple calls against one upstream server, especially stateful/session tools. By default both refuse to call a tool name that is not advertised by the server's current `tools/list`; pass `allowUnknownTool=true` only for an explicitly trusted dynamic server whose hidden tools are intentional.

## Resources and prompts

MCPace also proxies upstream prompts and resources when the upstream servers support them:

- `prompts/list` and `prompts/get` project upstream prompts with generated names.
- `resources/list`, `resources/templates/list`, and `resources/read` expose upstream resources through proxied MCPace URIs.

All dynamic list methods accept MCP cursors when pagination is enabled through env vars.

## Future token-reducer plugins

`tokenReducerPlugins` and `pluginPolicy` already provide a hook for future reducers. Built-in reducers remain deterministic and safe, while unknown reducers are ignored in best-effort mode and rejected only when strict mode is requested.

The intended plugin families are:

- content shaping: native, summary, compact, compat.
- diagnostic trimming: keep full data only on request.
- nested upstream de-duplication: preserve structured output, replace duplicate text payloads.
- large result offloading: return short summaries and MCP resource links for screenshots, DOM snapshots, logs, and large tables.
- tool-catalog ranking: dynamically expose the most relevant upstream tools for strict tool-budget clients.

## Design rule

Registries and brand-specific client catalogs are hints. The runtime truth is the active handshake, current transport, live upstream discovery, and the local policy/concurrency model.

## Large catalog hardening

For 50-server / 100k-200k-tool scenarios, see [Tool scale and reuse hardening](tool-scale-and-reuse-hardening.md).

See also: [Mixed upstream topology hardening](mixed-upstream-topologies.md).

See also: [Upstream fail-safe hardening](upstream-failsafe-hardening.md).
