# MCPace native token output shaping

MCPace is now native-first by default: wrapper tools keep actual upstream MCP `content`
items at the top level, put useful data in `structuredContent`, and avoid repeating bulky
MCPace lease/session diagnostics unless requested.

## Tool-result content modes

Use `resultMode` on `upstream_call`, `upstream_batch`, or `upstream_call` only when you
need to override the default:

- `native` — default; preserves upstream `content` items at the top level, propagates upstream `isError`, and compacts duplicate nested payloads.
- `summary` — returns short status text in `content[0].text` and keeps the result in `structuredContent`.
- `compact` — returns compact serialized JSON in `content[0].text` plus `structuredContent`.
- `compat` — legacy MCPace behavior; returns pretty serialized JSON in `content[0].text` plus `structuredContent`.

Environment override: `MCPACE_TOOL_RESULT_MODE=native|summary|compact|compat`.

## Upstream diagnostics modes

Use `diagnostics` only when you need MCPace internals:

- `none` — default; drops MCPace bridge diagnostics such as `lease*`, `sessionPool*`, `timeoutMs`, and `bridgeOk`.
- `summary` — keeps useful success/failure counters and booleans but drops bulky lease/session internals.
- `full` — legacy debugging mode; preserves all lease/session diagnostics.

Environment override: `MCPACE_UPSTREAM_DIAGNOSTICS=none|summary|full`.

## Nested upstream content de-duplication

`nestedContent: "compact"` is the default. MCPace keeps upstream `content` at the top
level in native mode, then replaces duplicated nested `content` text inside
`structuredContent` with a short marker when safe. Use `nestedContent: "full"` only for
low-level debugging.

Environment override: `MCPACE_NESTED_UPSTREAM_CONTENT=compact|full`.

## Built-in token reducer plugin hook

`tokenReducerPlugins` is a forward-compatible hook for future reducers. Current built-ins:

- `mcpace.native-content.v1` → `resultMode: "native"`
- `mcpace.summary-content.v1` → `resultMode: "summary"`
- `mcpace.compat-content.v1` → `resultMode: "compat"`
- `mcpace.compact-content.v1` → `resultMode: "compact"`
- `mcpace.trim-upstream-diagnostics.v1` → `diagnostics: "summary"`
- `mcpace.drop-upstream-diagnostics.v1` → `diagnostics: "none"`
- `mcpace.dedupe-nested-upstream-content.v1` → `nestedContent: "compact"`

Example debug override:

```json
{
  "server": "demo-server",
  "calls": [["demo_get_status", {}]],
  "resultMode": "compat",
  "diagnostics": "full",
  "nestedContent": "full"
}
```

## Native upstream output shaping

`upstream_call` is a promoted MCPace-native wrapper around the configured upstream MCP server
upstream. By default it captures `screenshot` and `text` without putting long JavaScript
snippets into the model prompt. Add `status` or `interestingElements` only when needed.

```json
{
  "include": ["screenshot", "text", "interestingElements"],
  "selector": "body",
  "maxElements": 100,
  "allowToolRiskClasses": true
}
```

`interestingElements` uses the existing `demo_script` upstream tool, so the same
policy guard applies: pass `allowToolRiskClasses: true` or an equivalent configured
risk-class opt-in.
