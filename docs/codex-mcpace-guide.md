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
Install writes create a local MCPace backup first; undo the latest Codex patch
with `mcpace client restore codex --backup latest --root .` if you need to roll
back to the previous config file.

The HTTP MCP endpoint exposes MCPace management and diagnostic tools. It also
provides explicit stdio upstream access through `surface_manifest`,
`upstream_catalog`, `upstream_probe`, `upstream_policy_audit`,
`upstream_policy_suggest`, `upstream_tools`, `upstream_call`, and
`upstream_batch`. These bridge tools
discover servers from
`mcp_settings.json` at runtime rather than hardcoding server names, so future
stdio MCP entries become probe/list/audit/call candidates as soon as their
command is installed and enabled. Real
`upstream_call` / `upstream_batch` executions now acquire a scheduler lease from
the planner-derived route, heartbeat-renew it when a call can outlive the
initial TTL, abort before accepting a result if the heartbeat loses ownership,
run the upstream MCP call, and release the lease before returning. Servers that
exist only in `mcp_settings.json` remain callable, but MCPace now assigns a
conservative single-writer `settings-only-conservative` request lease instead of
bypassing scheduling.

MCPace does not advertise fake direct tool names such as `browser` or
`read_file`, and it must not pretend that upstream tools are native top-level
MCPace tools. Call `surface_manifest` to see the exact contract: which tool
names are returned by MCPace `tools/list`, how configured upstream tools are
discovered, and why direct top-level projection is disabled by default. Call
`upstream_catalog` to get a flat `tools` array with concise server-qualified
tool descriptions and an `upstream_call`-ready `call` object for each
discovered tool. Call `upstream_policy_audit` when you need to compare
MCP ToolAnnotations, generic name heuristics, and configured MCPace
`toolPolicies` before exposing a new upstream server. Call
`upstream_policy_suggest` to turn unprotected guard-recommended audit findings
into copyable `toolPolicies` candidates. Use `upstream_tools` when you need one
server's full tool schemas, then `upstream_call` with the selected server and
tool name.
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
  `runtime_diagnostics`, `surface_manifest`, `upstream_catalog`,
  `upstream_probe`, `upstream_policy_audit`, `upstream_policy_suggest`,
  `upstream_tools`, `upstream_call`, `upstream_batch`, and `browser_status`.
- Calling an unsupported upstream name, such as `browser`, returns a normal MCP
  tool error payload. It must not close the HTTP transport.

This sequence specifically protects against the previous Codex startup failure:

```text
Transport channel closed, when send initialized notification
```

## Verify stdio upstream tools

Use `surface_manifest`, `upstream_catalog`, `upstream_probe`,
`upstream_policy_audit`, `upstream_policy_suggest`, `upstream_tools`,
`upstream_call`, and
`upstream_batch` to prove that MCPace can launch and call configured stdio
upstream MCP servers. Probe/catalog/audit/suggest/list
refreshes still perform short `tools/list` checks and share the same successful
`tools/list` cache; pass `"refresh": true` when you need a fresh live upstream
check rather than cached proof. Runtime calls through `upstream_call` and
`upstream_batch` keep the small wrapper surface but now use a bounded in-process
upstream session pool keyed by server, settings fingerprint, project root,
client/session context, transport, and metadata. Pool hits reuse an
already-initialized stdio upstream process instead of paying a fresh initialize
cost every call.

For HTTP MCP callers, `upstream_call` / `upstream_batch` derive the routing
context from explicit tool arguments first (`clientId`, `sessionId`,
`projectRoot`, `transport`), then from `metadata` hints
(`metadata.session.id`, `metadata.sessionId`, `metadata.clientId`, etc.), and
then from MCP/bridge headers such as `Mcp-Session-Id`,
`X-MCPace-Session-Id`, `X-Codex-Session-Id`, `X-MCPace-Client-Id`, and
`X-Codex-Client-Id`. Pass a stable `sessionId` for stateful tools when the
client does not supply one automatically; different chat/session ids should
create different pool keys, while the same id keeps session affinity.

### Automatic MCP hints vs explicit MCPace policy

MCPace does not hardcode special cases for popular servers, but it also does
not pretend the MCP protocol can prove everything automatically. In practice:

- if an upstream tool advertises trusted ToolAnnotations, MCPace can surface
  those hints for operator decisions;
- if a client sends roots/project/session metadata, MCPace can derive the
  appropriate project or session key;
- if the server is mutable, host-global, or lacks trusted annotations, the
  safe policy must be declared in `mcpace.config.json`.

`upstream_policy_audit` is the safe discovery loop for new MCPs: it launches the
same configured stdio server, reads `tools/list`, reports annotation keys,
advisory risk classes, matching `toolPolicies`, unprotected guard-recommended
tools, and unknown/unannotated tools. The audit is intentionally advisory; the
runtime enforces only declarative `toolPolicies`, not Rust-side hardcoded lists
or fragile guesses.

`upstream_policy_suggest` is the automation layer on top of the audit. It groups
unprotected guard-recommended tools by risk class, generates stable
`riskClass`/`allowArgument` names such as `browser-control` /
`allowBrowserControl` or `<server>-mutation` /
`allow<Server>Mutation`, and returns copyable policy snippets plus evidence.
It is dry-run by design: MCPace can generate the pattern automatically, but a
config update must still be explicit so heuristics never silently weaken or
change the user's policy.

Current local probes showed this matters: `context7` exposes `readOnlyHint`
annotations, while the installed `sequential-thinking`, `memory`, `filesystem`,
and `fetch` servers did not expose annotations through `tools/list`. MCPace
therefore keeps the stateful reference servers conservative by policy:

- `sequential-thinking`: `single-session` + `chat-session`, so two chats do not
  interleave one reasoning chain. Pass a stable `sessionId` or ensure the client
  forwards `Mcp-Session-Id`.
- `memory`: `single-writer` over `runtime-memory`; graph mutation tools require
  `allowMemoryMutation` or `allowToolRiskClasses:["memory-mutation"]` because
  the backing memory is intentionally persistent/cross-chat.
- `filesystem`: `single-writer` over `workspace-roots`; mutating tools require
  `allowFilesystemMutation` or
  `allowToolRiskClasses:["filesystem-mutation"]`. Read/list/search tools remain
  callable without that opt-in.
- `lean-ctx`: project-local context/shell tools stay serialized by project;
  `ctx_edit` requires `allowArguments:["allowLeanMutation"]` or
  `allowToolRiskClasses:["lean-mutation"]`, and `ctx_shell` requires
  `allowArguments:["allowLeanShell"]` or
  `allowToolRiskClasses:["lean-shell"]`.
- `serena`: project index/search/read tools remain available, while source-edit
  tools require `allowArguments:["allowCodeMutation"]` or
  `allowToolRiskClasses:["code-mutation"]`, and Serena memory mutation tools
  require `allowArguments:["allowSerenaMemoryMutation"]` or
  `allowToolRiskClasses:["serena-memory-mutation"]`.

Additional canary integrations are wired so they can be evaluated without
turning the whole workstation into an unbounded MCP surface:

- `browser` uses Agent Browser Protocol as the always-available host browser
  bridge. Read/status tools stay available, while browser-control tools such as
  action, scroll, navigation, JavaScript, dialogs, downloads/files, selectors,
  sliders, tabs, and permissions require `allowBrowserControl` or
  `allowToolRiskClasses:["browser-control"]`.
- `time` is enabled as a safe default canary (`get_current_time`,
  `convert_time`) because live probing showed it starts cleanly and has no
  mutation surface.
- `git`, `everything`, `sqlite`, and `playwright` are source-enabled but remain
  profile-gated under `labs` unless another profile explicitly enables them.
  Live canary probes confirmed their `tools/list` handshakes on this Windows
  host.
- `git` mutation tools (`git_add`, `git_commit`, `git_reset`,
  `git_create_branch`, `git_checkout`) require `allowGitMutation` or
  `allowToolRiskClasses:["git-mutation"]`.
- `sqlite` mutation tools (`write_query`, `create_table`, `append_insight`)
  require `allowSqliteMutation` or
  `allowToolRiskClasses:["sqlite-mutation"]`.
- `playwright` advertises useful ToolAnnotations, but MCPace still keeps
  state-changing browser tools behind declarative policy:
  `allowBrowserControl` or `allowToolRiskClasses:["browser-control"]`.
  Read-only Playwright tools such as snapshots, screenshots, console, network,
  and waits remain ungated.

### Config-driven risk gates for desktop/system MCP tools

`windows-mcp` is deliberately routed through the same `upstream_call` /
`upstream_batch` bridge instead of advertising every desktop tool as a native
top-level MCPace tool. That keeps the Codex-visible tool surface small and lets
MCPace attach the desktop host lock before the upstream tool runs.

Sensitive-tool gates are not hardcoded into MCPace's Rust code. They are
declared per upstream server in `mcpace.config.json` as `toolPolicies`, and the
same mechanism can be used for other MCP servers. Each policy declares:

- `tools`: exact tool names or simple `*` wildcard patterns;
- `riskClass`: a normalized risk class such as `desktop-observation`;
- `allowArgument`: an optional convenience boolean argument, such as
  `allowDesktopObservation`;
- `description`: an operator-facing reason shown in blocked-call errors.

For the current `windows-mcp` config:

- observation tools (`Snapshot`, `Screenshot`, `Scrape`) require
  either `"allowDesktopObservation": true` or
  `"allowToolRiskClasses": ["desktop-observation"]`;
- desktop-control tools (`App`, `Click`, `Type`, `Scroll`, `Move`, `Shortcut`,
  `MultiSelect`, `MultiEdit`, `Notification`) require
  either `"allowDesktopControl": true` or
  `"allowToolRiskClasses": ["desktop-control"]`;
- system-control tools (`PowerShell`, `FileSystem`, `Clipboard`, `Process`,
  `Registry`) require either `"allowSystemControl": true` or
  `"allowToolRiskClasses": ["system-control"]`;
- `Wait` remains safe and does not need an opt-in flag.

Examples:

```powershell
$body = '{"jsonrpc":"2.0","id":71,"method":"tools/call","params":{"name":"upstream_call","arguments":{"server":"windows-mcp","tool":"Screenshot","arguments":{"use_annotation":false},"allowDesktopObservation":true,"sessionId":"desktop-observe"}}}'
Invoke-RestMethod -Uri 'http://127.0.0.1:39022/mcp' -Method Post -Headers $headers -ContentType 'application/json' -Body $body

$body = '{"jsonrpc":"2.0","id":72,"method":"tools/call","params":{"name":"upstream_call","arguments":{"server":"windows-mcp","tool":"PowerShell","arguments":{"command":"Write-Output ''mcpace-ok''","timeout":10},"allowSystemControl":true,"sessionId":"desktop-system"}}}'
Invoke-RestMethod -Uri 'http://127.0.0.1:39022/mcp' -Method Post -Headers $headers -ContentType 'application/json' -Body $body

$body = '{"jsonrpc":"2.0","id":73,"method":"tools/call","params":{"name":"upstream_call","arguments":{"server":"windows-mcp","tool":"Type","arguments":{"text":"MCPACE_KEYBOARD_OK","press_enter":false},"allowDesktopControl":true,"sessionId":"desktop-control"}}}'
Invoke-RestMethod -Uri 'http://127.0.0.1:39022/mcp' -Method Post -Headers $headers -ContentType 'application/json' -Body $body
```

Equivalent generic risk-class authorization:

```powershell
$body = '{"jsonrpc":"2.0","id":74,"method":"tools/call","params":{"name":"upstream_call","arguments":{"server":"windows-mcp","tool":"Screenshot","arguments":{"use_annotation":false},"allowToolRiskClasses":["desktop-observation"],"sessionId":"desktop-observe"}}}'
Invoke-RestMethod -Uri 'http://127.0.0.1:39022/mcp' -Method Post -Headers $headers -ContentType 'application/json' -Body $body
```

For custom policy names that are not first-class schema fields, pass the
declared `allowArgument` through the generic `allowArguments` array:

```json
{
  "allowArguments": ["allowCustomRisk"]
}
```

For keyboard tools, first focus a known test window (for example a temporary
Notepad file launched by a guarded `PowerShell` call) and clean it up by process
id. Do not type into an arbitrary active desktop window.

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

Audit annotations and declarative policy coverage for every configured
upstream. Use this before adding new popular MCPs or before enabling a
profile-gated server by default:

```powershell
$body = '{"jsonrpc":"2.0","id":44,"method":"tools/call","params":{"name":"upstream_policy_audit","arguments":{"refresh":true,"timeoutMs":45000}}}'
Invoke-RestMethod `
  -Uri 'http://127.0.0.1:39022/mcp' `
  -Method Post `
  -Headers $headers `
  -ContentType 'application/json' `
  -Body $body
```

Generate copyable policy candidates from the same audit signals:

```powershell
$body = '{"jsonrpc":"2.0","id":45,"method":"tools/call","params":{"name":"upstream_policy_suggest","arguments":{"refresh":true,"timeoutMs":45000}}}'
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
$body = '{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"upstream_call","arguments":{"server":"filesystem","tool":"list_allowed_directories","arguments":{},"timeoutMs":120000,"sessionId":"docs-smoke-session"}}}'
Invoke-RestMethod `
  -Uri 'http://127.0.0.1:39022/mcp' `
  -Method Post `
  -Headers $headers `
  -ContentType 'application/json' `
  -Body $body
```

Call a stateful browser sequence in one upstream session:

```powershell
$body = '{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"upstream_batch","arguments":{"server":"browser","timeoutMs":180000,"sessionId":"docs-browser-session","allowBrowserControl":true,"calls":[{"tool":"browser_navigate","arguments":{"url":"data:text/html,<title>MCPace</title><main>MCPace browser batch ok</main>"}},{"tool":"browser_text","arguments":{}},{"tool":"browser_shutdown","arguments":{"timeout_ms":3000}}]}}}'
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
  invalid tool arguments do not look like successful real work. Declared MCPace
  servers also include `leaseAttached`, `leaseId`, `leaseReleased`, `lease`, and
  `leaseRelease` fields so you can see the request-time scheduler gate. Long
  calls with a short lease TTL also report `leaseHeartbeatStarted` and
  `leaseHeartbeatRenewalCount`; lost-heartbeat diagnostics use
  `leaseHeartbeatLost` / `leaseHeartbeatFailureCount` before the bridge returns
  a result. Pooled calls also report `sessionPoolEnabled`,
  `sessionPoolHit`, `sessionPoolSessionCallCount`, and `sessionPoolSize` so you
  can tell whether an upstream process was reused.
- `upstream_batch` returns one `results` item per requested upstream tool while
  keeping state within that batch and, when the pool key matches, across later
  calls or batches in the same MCPace process. This is the preferred browser
  automation path when a follow-up action depends on the page opened by a
  previous action. The whole batch holds one scheduler lease, heartbeat-renews
  it if needed, and releases it before responding.
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
their schemas. This is an explicit proxy/wrapper contract, not a trick:
`surface_manifest` reports the exact top-level MCPace `tools/list` names and
can optionally include the live upstream catalog. `upstream_catalog` is the
native discovery surface for upstream tools: it returns a flat server-qualified
catalog for immediate selection while preserving grouped server diagnostics.
The short `tools/list` cache makes repeated
probe/catalog/list calls cheap while still invalidating on `mcp_settings.json`
metadata changes and allowing explicit `refresh`.

The current wrapper tools use request-time scheduler leases and a first bounded
session-pool slice. The remaining connector-manager work is to harden that pool
into a full runtime owner:

1. keep warm upstream sessions keyed by server, client/session affinity, project
   root, config fingerprint, metadata, and route/process scope;
2. apply bounded concurrency and backpressure per upstream;
3. renew leases while sessions are active and invalidate sessions on lease
   expiry, config changes, protocol errors, and
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

## Source-regression coverage added on April 28, 2026

Rust tests now cover the request-time lease gate for explicit upstream wrapper
calls:

- successful stdio fallback `upstream_call` attaches and releases a scheduler
  lease, leaving `activeLeaseCount: 0`;
- settings-only upstream servers get a conservative single-writer request lease
  instead of a scheduler bypass;
- a short-TTL upstream call heartbeat-renews its lease while the fake upstream
  delays the response;
- a forced lost lease cancels the in-flight upstream wait and refuses the stale
  successful result;
- a conflicting held lease blocks `upstream_call` before the fake upstream
  process launches;
- stale JSON-RPC response ids from an upstream helper are ignored in favor of the
  expected request id;
- the HTTP tool dispatch path uses the same upstream lease context and releases
  its lease after the call.
- `upstream_probe` across all enabled upstreams, including `lean-ctx` after the
  `lean-ctx` binary was installed and resolved from PATH
- `upstream_policy_audit` policy/annotation review for configured upstream
  servers, including live canary MCPs and disabled/profile-gated entries
- `upstream_call` safe smoke calls for those same stdio servers
- `browser_status`, `upstream_tools server=browser`, and
  `upstream_call server=browser tool=browser_get_status`
- `upstream_batch server=browser` for `browser_navigate -> browser_text ->
  browser_shutdown`
- `/healthz`

The verified running endpoint was `http://127.0.0.1:39022/mcp`.
