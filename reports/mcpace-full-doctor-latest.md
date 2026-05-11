# MCPace full doctor report

Generated: 2026-05-11T12:13:27.229Z
Root: `C:\Users\rmatv\Projects\mcpace`

Status: **WARN**

Checks: pass 13, warn 12, fail 0, skip 8.

## ✅ platform.identity

**Platform: win32/x64**

## ✅ tool.node

**node found**

```text
C:\Program Files\nodejs\node.EXE
```

## ✅ tool.npm

**npm found**

```text
C:\Program Files\nodejs\npm.CMD
```

## ✅ tool.npx

**npx found**

```text
C:\Program Files\nodejs\npx.CMD
```

## ✅ tool.where

**where found**

```text
C:\Windows\system32\where.EXE
```

## ✅ source.root

**Root directory exists**

```text
C:\Users\rmatv\Projects\mcpace
```

## ✅ mcpace.binary

**mcpace binary found and executable**

```text
C:\Users\rmatv\Projects\mcpace\target\release\mcpace.exe
```

## ✅ mcpace.help

**mcpace --help works**

```text
MCPace Rust-only local MCP hub

Implemented now:
  version
  doctor [--json] [--root <path>]
  setup [--json] [--root <path>] [--host <addr>] [--port <n>] [--max-connections <n>] [--io-timeout-ms <n>] [--max-body-bytes <n>] [--overview-cache-ms <n>] [--skip-client-install] [--install-service|--install-autostart] [--no-enable]
  service install|status|uninstall|print [--json] [--root <path>] [--host <addr>] [--port <n>] [--max-connections <n>] [--io-timeout-ms <n>] [--max-body-bytes <n>] [--overview-cache-ms <n>] [--dry-run] [--no-enable]
  dashboard [--root <path>] [--host <addr>] [--port <n>] [--max-connections <n>] [--io-timeout-ms <n>] [--max-body-bytes <n>] [--overview-cache-ms <n>]
  serve [--root <path>] [--host <addr>] [--port <n>] [--max-connections <n>] [--io-timeout-ms <n>] [--max-body-bytes <n>] [--overview-cache-ms <n>]
  serve start|stop|status [--json] [--root <path>] [--host <addr>] [--port <n>] [--max-connections <n>] [--io-timeout-ms <n>] [--max-body-bytes <n>] [--overview-cache-ms <n>]
  local HTTP defaults: max connections=8, IO timeout=30000ms, max body=1048576 bytes, overview cache=1500ms, health cache=1000ms
  init [--json] [--root <path>]
  hub up [--json] [--root <path>] [--foreground]
  hub down [--json] [--root <path>]
  hub repair [--json] [--root <path>]
  hub status [--json] [--root <path>]
  hub logs [--json] [--root <path>] [--tail <n>]
  hub lease list [--json] [--root <path>]
  hub lease acquire --server <name> [--json] [--root <path>] [--client-id <id>] [--session-id <id>] [--project-root <path>] [--ttl-ms <n>]
  hub lease renew --lease-id <id> [--json] [--root <path>] [--ttl-ms <n>]
  hub lease release --lease-id <id> [--json] [--root <path>]
  profile [show] [--json] [--root <path>]
  projects [list] [--json] [--root <path>]
  candidates [--json] [--root <path>]
  server presets [--json] [--root <path>]
  server install <preset> [--path <path>...] [--arg <arg>...] [--env KEY=VALUE...] [--json] [--root <path>] [--dry-run] [--force]
  server starter [--path <path>...] [--json] [--root <path>] [--dry-run] [--force]
  connect [<client>] [--server <name>] [--json] [--root <path>]
  client list [--json] [--root <path>]
  client plan [--json] [--root <path>] [--client-id <id>] [--session-id <id>] [--project-root <path>] [--transport <stdio|streamable-http>]
  client install <client|all> [--json] [--root <path>] [--dry-run] [--diff]
  client restore <client|all> [--json] [--root <path>] [--backup <id|latest>]
  client export <client> [--json] [--root <path>] [--transport <stdio|streamable-http>] [--session-id <id>] [--project-root <path>]
  mcp-server [--root <path>] [--client-id <id>] [--session-id <id>] [--project-root <path>] [--transport <stdio|streamable-http>]  # internal compatibility
  stdio-shim --json [--root <path>] [--client-id <id>] [--session-id <id>] [--project-root <path>] [--transport <stdio|streamable-http>] [--metadata-json <json>]  # internal bootstrap proof
  lab list [--json] [--root <path>]
  lab matrix [--json] [--root <path>]
  lab coverage [--json] [--root <path>]
  lab gaps [--json] [--root <path>]
  lab report [--json] [--root <path>]
  lab show --id <scenario> [--json] [--root <path>]
  server list [--json] [--root <path>]
  server capabilities [--json] [--root <path>] [--name <server>]
  server sources [--json] [--root <path>]
  server presets [--json] [--root <path>]
  server install <preset> [--path <path>...] [--arg <arg>...] [--env KEY=VALUE...] [--settings <path>] [--dry-run] [--force] [--json] [--root <path>]
  server starter [--path <path>...] [--settings <path>] [--dry-run] [--force] [--json] [--root <path>]
  server test [<name>|--name <server>] [--timeout-ms <ms>] [--refresh] [--json] [--root <path>]
  server candidates [--json] [--root <path>]
  server add <name> --command <cmd> [--arg <arg>...] [--env KEY=VALUE...] [--settings <path>] [--dry-run] [--force] [--json]
  server add <name> --url <url> [--type http|streamable-http] [--header KEY=VALUE...] [--settings <path>] [--dry-run] [--force] [--json]
  server import --from <mcp-settings.json> [--settings <target.json>] [--dry-run] [--force] [--json]
  server remove <name> [--settings <path>] [--dry-run] [--json]
  server enable|disable <name> [--settings <path>] [--dry-run] [--json]
  verify doctor [--json] [--root <path>]
  verify readiness [--json] [--root <path>]
  repair [--json] [--root <path>]
  release [build] [--json] [--root <path>]
  update check [--json] [--source none|env|npm] [--latest-version <semver>] [--package <name>]

doctor/profile/projects/candidates/connect/client-plan/lab/server/verify have native Rust read paths; connect gives a client-first read-only wiring guide across endpoint, client target, upstream sources, readiness blockers, and exact next commands; server sources inventories every MCP settings source, server presets/install/starter add useful MCPs without memorizing package args, server add writes per-server fragments under mcp_settings.d/, server import copies existing mcpServers blocks into MCPace fragments, server enable/disable toggles a BYO MCP entry without deleting it, server remove deletes stale BYO MCP entries without manual JSON editing, and server test probes configured upstreams before clients use them; setup starts the one-port MCPace endpoint, installs supported local clients, and smokes the configured health plus MCP paths in one command; service installs user-level autostart entries without requiring mcpace in PATH; serve is the public one-port MCPace surface on http://127.0.0.1:39022/mcp and now has start/stop/status lifecycle commands, dashboard provides the same local web control surface, init seeds the runtime layout, hub owns a local lifecycle/state/log/repair/lease surface, client install patches MCPace entries for catalog-declared local patchers (codex, claude-code, cursor-local, kiro-ide, kiro-cli, windsurf, gemini-cli, github-copilot-cli, and hermes-agent) and client install all can patch every supported local tar
```

## ✅ mcpace.version

**mcpace version works**

```text
0.5.9
```

## ✅ mcpace.doctor

**mcpace doctor works**

```text
{
  "project": {
    "cargoManifestFound": true,
    "clientConfigWarnings": [],
    "configFound": true,
    "configVersion": "0.5.9",
    "containerToolingReady": true,
    "missingRuntimePrerequisites": [],
    "npmSurfaceReady": true,
    "npmWorkspaceFound": true,
    "releaseManifestFound": true,
    "rootPath": "C:\\Users\\rmatv\\Projects\\mcpace",
    "runtimePrerequisites": [
      {
        "found": true,
        "name": "lean-ctx",
        "reasons": [
          "enabled stdio source command for server 'lean-ctx'"
        ]
      }
,      {
        "found": true,
        "name": "npx",
        "reasons": [
          "enabled stdio source command for server 'browser'"
,          "enabled stdio source command for server 'context7'"
,          "enabled stdio source command for server 'everything'"
,          "enabled stdio source command for server 'exa'"
,          "enabled stdio source command for server 'filesystem'"
,          "enabled stdio source command for server 'memory'"
,          "enabled stdio source command for server 'playwright'"
,          "enabled stdio source command for server 'sequential-thinking'"
        ]
      }
,      {
        "found": true,
        "name": "uvx",
        "reasons": [
          "enabled stdio source command for server 'fetch'"
,          "enabled stdio source command for server 'git'"
,          "enabled stdio source command for server 'serena'"
,          "enabled stdio source command for server 'sqlite'"
,          "enabled stdio source command for server 'time'"
,          "enabled stdio source command for server 'windows-mcp'"
,          "enabled stdio source command for server 'wireshark-mcp'"
        ]
      }
    ],
    "runtimePrerequisitesReady": true,
    "rustSourceReady": true
  },
  "tools": [
    {
      "found": true,
      "name": "cargo",
      "required": true,
      "version": "cargo 1.95.0 (f2d3ce0bd 2026-03-21)"
    }
,    {
      "found": true,
      "name": "rustc",
      "required": true,
      "version": "rustc 1.95.0 (59807616e 2026-04-14)"
    }
,    {
      "found": true,
      "name": "node",
      "required": true,
      "version": "v24.15.0"
    }
,    {
      "found": true,
      "name": "npm",
      "required": true,
      "version": "11.4.2"
    }
,    {
      "found": true,
      "name": "docker",
      "required": false,
      "version": "Docker version 29.3.1, build c2be9cc"
    }
  ]
}
```

## ✅ config.discovery

**Found 2 MCP settings file(s)**

```text
C:\Users\rmatv\Projects\mcpace\mcp_settings.json
C:\Users\rmatv\.mcpace\mcp_settings.d\restored-from-mcpace-history-72d64b0.json
```

## ✅ config.file:C:\Users\rmatv\Projects\mcpace\mcp_settings.json

**Config loaded: 0 server(s)**

## ✅ config.file:C:\Users\rmatv\.mcpace\mcp_settings.d\restored-from-mcpace-history-72d64b0.json

**Config loaded: 24 server(s)**

## ⚠️ server.npx-env:browser

**npx server is missing env_vars passthrough: browser**

```text
NPM_CONFIG_REGISTRY, NPM_CONFIG_USERCONFIG, NPM_CONFIG_GLOBALCONFIG, NPM_CONFIG_CACHE, NODE_EXTRA_CA_CERTS, SSL_CERT_FILE, REQUESTS_CA_BUNDLE, HTTP_PROXY, HTTPS_PROXY, NO_PROXY, http_proxy, https_proxy, no_proxy, CI
```

## ⚠️ server.npx-env:filesystem

**npx server is missing env_vars passthrough: filesystem**

```text
NPM_CONFIG_REGISTRY, NPM_CONFIG_USERCONFIG, NPM_CONFIG_GLOBALCONFIG, NPM_CONFIG_CACHE, NODE_EXTRA_CA_CERTS, SSL_CERT_FILE, REQUESTS_CA_BUNDLE, HTTP_PROXY, HTTPS_PROXY, NO_PROXY, http_proxy, https_proxy, no_proxy, CI
```

## ⚠️ server.npx-env:memory

**npx server is missing env_vars passthrough: memory**

```text
NPM_CONFIG_REGISTRY, NPM_CONFIG_USERCONFIG, NPM_CONFIG_GLOBALCONFIG, NPM_CONFIG_CACHE, NODE_EXTRA_CA_CERTS, SSL_CERT_FILE, REQUESTS_CA_BUNDLE, HTTP_PROXY, HTTPS_PROXY, NO_PROXY, http_proxy, https_proxy, no_proxy, CI
```

## ⚠️ server.npx-env:sequential-thinking

**npx server is missing env_vars passthrough: sequential-thinking**

```text
NPM_CONFIG_REGISTRY, NPM_CONFIG_USERCONFIG, NPM_CONFIG_GLOBALCONFIG, NPM_CONFIG_CACHE, NODE_EXTRA_CA_CERTS, SSL_CERT_FILE, REQUESTS_CA_BUNDLE, HTTP_PROXY, HTTPS_PROXY, NO_PROXY, http_proxy, https_proxy, no_proxy, CI
```

## ⚠️ server.npx-env:context7

**npx server is missing env_vars passthrough: context7**

```text
NPM_CONFIG_REGISTRY, NPM_CONFIG_USERCONFIG, NPM_CONFIG_GLOBALCONFIG, NPM_CONFIG_CACHE, NODE_EXTRA_CA_CERTS, SSL_CERT_FILE, REQUESTS_CA_BUNDLE, HTTP_PROXY, HTTPS_PROXY, NO_PROXY, http_proxy, https_proxy, no_proxy, CI
```

## ⚠️ server.api-env:context7

**Server may need API env_vars: context7**

```text
CONTEXT7_API_KEY
```

## ⏭️ server.disabled:github

**Server disabled: github**

## ⏭️ server.disabled:sentry

**Server disabled: sentry**

## ⚠️ server.api-env:serena

**Server may need API env_vars: serena**

```text
GITHUB_TOKEN, GITHUB_PERSONAL_ACCESS_TOKEN
```

## ⚠️ server.serena-project:serena

**Serena project root was not found on this machine**

```text
ide
```

## ⏭️ server.disabled:screenpipe

**Server disabled: screenpipe**

## ⏭️ server.disabled:firecrawl

**Server disabled: firecrawl**

## ⚠️ server.npx-env:exa

**npx server is missing env_vars passthrough: exa**

```text
NPM_CONFIG_REGISTRY, NPM_CONFIG_USERCONFIG, NPM_CONFIG_GLOBALCONFIG, NPM_CONFIG_CACHE, NODE_EXTRA_CA_CERTS, SSL_CERT_FILE, REQUESTS_CA_BUNDLE, HTTP_PROXY, HTTPS_PROXY, NO_PROXY, http_proxy, https_proxy, no_proxy, CI
```

## ⚠️ server.api-env:exa

**Server may need API env_vars: exa**

```text
EXA_API_KEY
```

## ⏭️ server.disabled:pdf

**Server disabled: pdf**

## ⏭️ server.disabled:postgres

**Server disabled: postgres**

## ⏭️ server.disabled:brave-search

**Server disabled: brave-search**

## ⏭️ server.disabled:notion

**Server disabled: notion**

## ⚠️ server.npx-env:playwright

**npx server is missing env_vars passthrough: playwright**

```text
NPM_CONFIG_REGISTRY, NPM_CONFIG_USERCONFIG, NPM_CONFIG_GLOBALCONFIG, NPM_CONFIG_CACHE, NODE_EXTRA_CA_CERTS, SSL_CERT_FILE, REQUESTS_CA_BUNDLE, HTTP_PROXY, HTTPS_PROXY, NO_PROXY, http_proxy, https_proxy, no_proxy, CI
```

## ⚠️ server.npx-env:everything

**npx server is missing env_vars passthrough: everything**

```text
NPM_CONFIG_REGISTRY, NPM_CONFIG_USERCONFIG, NPM_CONFIG_GLOBALCONFIG, NPM_CONFIG_CACHE, NODE_EXTRA_CA_CERTS, SSL_CERT_FILE, REQUESTS_CA_BUNDLE, HTTP_PROXY, HTTPS_PROXY, NO_PROXY, http_proxy, https_proxy, no_proxy, CI
```
