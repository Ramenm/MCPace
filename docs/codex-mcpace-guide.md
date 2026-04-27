# Codex and MCPace local MCP guide

This guide records the supported local Codex integration for `MCPace`. Use it to
install the Codex config block, start the local MCP endpoint, verify the
Streamable HTTP handshake, and clean up temporary checks without stopping the
intended MCPace background server.

## Current contract

MCPace exposes one local Streamable HTTP MCP endpoint for Codex:

```text
http://127.0.0.1:39022/mcp
```

The expected Codex config block is:

```toml
[mcp_servers.MCPace]
url = "http://127.0.0.1:39022/mcp"
enabled = true
startup_timeout_sec = 20
```

`MCPace` owns this block when you install it through
`mcpace client install codex` or `mcpace client install all`.

The HTTP MCP endpoint exposes MCPace management and diagnostic tools. It also
provides explicit stdio upstream access through `upstream_catalog`,
`upstream_probe`, `upstream_tools`, `upstream_call`, and `upstream_batch`. These
bridge tools discover servers from `mcp_settings.json` at runtime rather than
hardcoding server names, so future stdio MCP entries become probe/list/call
candidates as soon as their command is installed and enabled.

MCPace does not advertise fake direct tool names such as `browser` or
`read_file`. Call `upstream_catalog` to get a flat `tools` array with concise
server-qualified tool descriptions and an `upstream_call`-ready `call` object
for each discovered tool. Use `upstream_tools` when you need one server's full
tool schemas, then `upstream_call` with the selected server and tool name.
`upstream_catalog` and `upstream_tools` use a short in-process `tools/list` cache; pass
`"refresh": true` when you need to force a fresh upstream schema after changing
configuration or upgrading an upstream package. For stateful upstreams such as
browsers, use `upstream_batch` so navigation and follow-up reads run inside one
initialized upstream session and no helper process is left behind. The browser entry
uses Agent Browser Protocol (`agent-browser-protocol@0.1.10 --mcp`) as a
host-side stdio MCP server with `ABP_HEADLESS=0`, so Windows/macOS/Linux use a
real visible host browser rather than a fake/headless Playwright placeholder.
Other HTTP-only host-bridge entries remain diagnostics-only until a real bridge
or proxy is configured.

## Start or repair the local endpoint

Run these commands from the repository root after building the release binary:

```powershell
cargo +stable build --release
.\target\release\mcpace.exe setup --json --root . --host 127.0.0.1 --port 39022 --skip-client-install
```

Use `--skip-client-install` when the Codex block is already present and you only
need to start and smoke-test the endpoint. If the Codex block is missing, run:

```powershell
.\target\release\mcpace.exe client install codex --json --root .
```

## Verify Codex can see the MCP server config

Check the configured MCP servers:

```powershell
codex mcp list --json
```

The `MCPace` entry must be enabled, and its transport must be
`streamable_http` with the URL `http://127.0.0.1:39022/mcp`.

## Verify the Streamable HTTP handshake

The Codex RMCP client performs more than a simple `tools/list` probe. Verify the
full sequence that matters:

```powershell
$headers = @{ Accept = 'application/json, text/event-stream' }

$initialize = '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"codex-rmcp-smoke","version":"0.1"}}}'
Invoke-WebRequest `
  -Uri 'http://127.0.0.1:39022/mcp' `
  -Method Post `
  -Headers $headers `
  -ContentType 'application/json' `
  -Body $initialize

$initialized = '{"jsonrpc":"2.0","method":"notifications/initialized"}'
Invoke-WebRequest `
  -Uri 'http://127.0.0.1:39022/mcp' `
  -Method Post `
  -Headers $headers `
  -ContentType 'application/json' `
  -Body $initialized

$tools = '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}'
Invoke-WebRequest `
  -Uri 'http://127.0.0.1:39022/mcp' `
  -Method Post `
  -Headers $headers `
  -ContentType 'application/json' `
  -Body $tools
```

Expected results:

- `initialize` returns `200 OK` with a `protocolVersion`.
- `notifications/initialized` returns `202 Accepted` with an empty body.
- `tools/list` returns `200 OK` with MCPace management tools, including
  `runtime_diagnostics`, `upstream_catalog`, `upstream_probe`,
  `upstream_tools`, `upstream_call`, `upstream_batch`, and `browser_status`.
- Calling an unsupported upstream name, such as `browser`, returns a normal MCP
  tool error payload. It must not close the HTTP transport.

This sequence specifically protects against the previous Codex startup failure:

```text
Transport channel closed, when send initialized notification
```

## Verify stdio upstream tools

Use `upstream_catalog`, `upstream_probe`, `upstream_tools`, `upstream_call`, and
`upstream_batch` to prove that MCPace can launch and call configured stdio
upstream MCP servers. Each uncached launch starts the selected upstream hidden
on Windows, performs a short JSON-RPC session, returns the result, and cleans up
the helper process. `upstream_probe`, `upstream_catalog`, and `upstream_tools`
share the same short successful `tools/list` cache; pass `"refresh": true` when
you need a fresh live upstream check rather than cached proof.

Catalog configured upstream tool names and short descriptions without hardcoded
names. The response keeps the grouped `servers` details for diagnostics and also
returns top-level `tools[]` entries shaped like
`{ server, name, qualifiedName, title, description, call }`, where `call` can be
passed directly to `tools/call` as `upstream_call`:

```powershell
$body = '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"upstream_catalog","arguments":{"timeoutMs":45000}}}'
Invoke-RestMethod `
  -Uri 'http://127.0.0.1:39022/mcp' `
  -Method Post `
  -Headers $headers `
  -ContentType 'application/json' `
  -Body $body
```

Force-refresh cached tool schemas after a config/package change:

```powershell
$body = '{"jsonrpc":"2.0","id":33,"method":"tools/call","params":{"name":"upstream_catalog","arguments":{"refresh":true,"timeoutMs":45000}}}'
Invoke-RestMethod `
  -Uri 'http://127.0.0.1:39022/mcp' `
  -Method Post `
  -Headers $headers `
  -ContentType 'application/json' `
  -Body $body
```

Probe every configured upstream without hardcoded names. Add `"refresh": true`
to force a fresh process launch and `tools/list` request after config/package
changes or when debugging live readiness:

```powershell
$body = '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"upstream_probe","arguments":{"timeoutMs":45000}}}'
Invoke-RestMethod `
  -Uri 'http://127.0.0.1:39022/mcp' `
  -Method Post `
  -Headers $headers `
  -ContentType 'application/json' `
  -Body $body
```

List an upstream server's tools:

```powershell
$body = '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"upstream_tools","arguments":{"server":"memory","timeoutMs":120000}}}'
Invoke-RestMethod `
  -Uri 'http://127.0.0.1:39022/mcp' `
  -Method Post `
  -Headers $headers `
  -ContentType 'application/json' `
  -Body $body
```

Call a safe upstream tool:

```powershell
$body = '{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"upstream_call","arguments":{"server":"filesystem","tool":"list_allowed_directories","arguments":{},"timeoutMs":120000}}}'
Invoke-RestMethod `
  -Uri 'http://127.0.0.1:39022/mcp' `
  -Method Post `
  -Headers $headers `
  -ContentType 'application/json' `
  -Body $body
```

Call a stateful browser sequence in one upstream session:

```powershell
$body = '{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"upstream_batch","arguments":{"server":"browser","timeoutMs":180000,"calls":[{"tool":"browser_navigate","arguments":{"url":"data:text/html,<title>MCPace</title><main>MCPace browser batch ok</main>"}},{"tool":"browser_text","arguments":{}},{"tool":"browser_shutdown","arguments":{"timeout_ms":3000}}]}}}'
Invoke-RestMethod `
  -Uri 'http://127.0.0.1:39022/mcp' `
  -Method Post `
  -Headers $headers `
  -ContentType 'application/json' `
  -Body $body
```

Expected results:

- `upstream_probe` reports one result per configured server and fails individual
  broken future servers cleanly, for example missing command binaries, without
  closing the MCP HTTP transport. Successful probe results include `cacheHit`
  and `cacheTtlMs`; the top-level response includes `cacheHitCount` /
  `cacheMissCount` so repeated probes can be fast without hiding how the result
  was obtained.
- `upstream_catalog` returns top-level flat `tools[]` entries plus grouped
  `servers[]` diagnostics for configured callable stdio servers and skips
  hardcoded server assumptions. Its response includes `cacheHit` per server and
  top-level `cacheHitCount` / `cacheMissCount` so clients can see when the short
  `tools/list` cache was reused.
- `upstream_tools` returns `isError: false` and a `tools` array for enabled
  stdio servers when their command is installed and resolvable. It includes
  `cacheHit` and `cacheTtlMs` so clients can decide when to pass
  `"refresh": true`.
- `upstream_call` returns `isError: false` and an `upstreamResult` object for
  successful upstream tool calls. Its payload separates bridge success from
  upstream tool success with `bridgeOk`, `upstreamOk`, and `upstreamIsError`, so
  invalid tool arguments do not look like successful real work.
- `upstream_batch` returns one `results` item per requested upstream tool while
  keeping state within that batch. This is the preferred browser automation path
  when a follow-up action depends on the page opened by a previous action.
- `browser_status` returns `status: callable-stdio-abp` when the ABP browser
  stdio bridge is configured.
- `upstream_tools` with `server: "browser"` returns the ABP browser tool list.
- `upstream_call` with `server: "browser"` and `tool: "browser_get_status"`
  proves the real host browser bridge can start. On Windows, the helper process
  is launched without a console window; the browser window itself may be visible
  because `ABP_HEADLESS=0` is intentional.

## Performance and native-routing direction

Current MCPace intentionally keeps upstream calls behind stable wrapper tools
instead of projecting every upstream tool directly into `tools/list`. This keeps
client startup fast and avoids breaking clients when upstream packages change
their schemas. `upstream_catalog` is the native discovery surface: it returns a
flat server-qualified catalog for immediate selection while preserving grouped
server diagnostics. The short `tools/list` cache makes repeated
probe/catalog/list calls cheap while still invalidating on `mcp_settings.json`
metadata changes and allowing explicit `refresh`.

The next larger performance step is a lease/route-scoped connector manager:

1. key warm upstream sessions by server, client/session affinity, project root,
   and route/process scope;
2. apply bounded concurrency and backpressure per upstream;
3. invalidate sessions on lease expiry, config changes, protocol errors, and
   explicit refresh;
4. keep wrapper tools as the compatibility baseline, then optionally add
   namespaced direct tool projection behind a feature flag.

Do not replace the wrapper tools with direct projection by default until the
connector manager exists; otherwise `tools/list` would need to launch arbitrary
upstreams during client startup.

## Verify the background server is intentional

Check server status:

```powershell
.\target\release\mcpace.exe serve status --json --root .
```

A running `mcpace-serve.exe` process on `127.0.0.1:39022` is expected. It is not
leftover test noise; it is the process Codex connects to. Stop it only when you
want MCPace to be unavailable to Codex:

```powershell
.\target\release\mcpace.exe serve stop --json --root .
```

On Windows, background `serve` and `hub` launches use a hidden detached process
creation path. The user-level autostart entry uses `wscript.exe` with a
generated `mcpace-autostart.vbs` launcher so it can start
`mcpace serve start` hidden. These paths must not open extra terminal windows.

## Restart Codex after changing the server

Codex reads and connects MCP servers during session startup. After changing the
MCPace binary, endpoint, or config block, start a new Codex session before
expecting `MCPace` tools to appear.

The already-open session might still show zero MCPace tools even when the HTTP
endpoint is healthy. Treat a new session as the source of truth for MCP tool
discovery.

## Troubleshooting

Use this checklist when Codex reports `MCP startup incomplete` for `MCPace`.

1. Confirm the listener:

   ```powershell
   Get-NetTCPConnection -LocalPort 39022 -State Listen
   ```

2. Confirm health:

   ```powershell
   Invoke-RestMethod -Uri 'http://127.0.0.1:39022/healthz'
   ```

3. Confirm the `notifications/initialized` response is `202 Accepted` with an
   empty body.

4. Confirm `codex mcp list --json` shows `MCPace` as enabled.

5. Restart Codex after any repair.

## Verified on April 27, 2026

The local Windows host verification covered:

- `cargo +stable check`
- `cargo +stable test --all-targets`
- `cargo +stable build --release`
- `npm test`
- `codex mcp list --json`
- `initialize -> notifications/initialized -> ping -> tools/list`
- `upstream_tools` for `memory`, `filesystem`, `sequential-thinking`,
  `context7`, `fetch`, `serena`, `exa`, and `wireshark-mcp`
- `upstream_probe` across all enabled upstreams, including `lean-ctx` after the
  `lean-ctx` binary was installed and resolved from PATH
- `upstream_call` safe smoke calls for those same stdio servers
- `browser_status`, `upstream_tools server=browser`, and
  `upstream_call server=browser tool=browser_get_status`
- `upstream_batch server=browser` for `browser_navigate -> browser_text ->
  browser_shutdown`
- `/healthz`

The verified running endpoint was `http://127.0.0.1:39022/mcp`.
