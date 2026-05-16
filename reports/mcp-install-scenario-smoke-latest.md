# MCP install scenario smoke

Generated: 2026-05-16T13:57:56.144Z
Status: pass
Project: mcpace 0.6.4

## Checks

| Check | Status | Detail |
|---|---:|---|
| preset-install-dry-run-is-config-only | pass | exit=0 30ms |
| preset-install-writes-one-fragment | pass | exit=0 23ms |
| preset-install-does-not-run-package-command | pass | Install output materialized command/args in JSON only; package execution is deferred until runtime/test/client launch. |
| reinstall-without-force-is-blocked | pass | exit=1 server 'filesystem' already exists in /tmp/mcpace-install-scenarios-CFLpea/mcp_settings.d/filesystem.json; rerun with --force to replace it |
| reinstall-with-force-replaces | pass | exit=0 24ms |
| custom-stdio-server-add | pass | exit=0 22ms |
| remote-http-server-add | pass | exit=0 20ms |
| invalid-remote-url-is-rejected | pass | exit=1 server add --url currently accepts only http:// or https:// MCP endpoints |
| paid-server-can-be-registered-disabled | pass | exit=0 21ms |
| hundred-server-config-scale | pass | 100 fragments written in 2143ms; inventory serverCount=104 |

## Scenario matrix

| Scenario | Expected behavior | Risk covered |
|---|---|---|
| preset install dry-run | No file is written; output says dry-run-add. | Prevents accidental config mutation while evaluating an MCP server. |
| preset install apply/reapply/force | First apply writes one fragment, second apply without --force fails, --force replaces. | Prevents hidden reinstall/duplicate drift and makes replacement explicit. |
| custom stdio server | Writes command/args only; does not execute the command during add. | Separates registration from runtime execution. |
| remote Streamable HTTP server | Accepts http(s) URL and headers as config; rejects non-http(s). | Separates remote domain ownership from local MCPace endpoint ownership. |
| paid/expensive server disabled by default | Entry can be added with enabled=false; costs remain dependent on later runtime/tool calls. | Avoids accidental activation while still allowing reviewable config. |
| 100-server config scale | 100 distinct fragments are written and visible in source inventory. | Covers many-server config fanout without claiming runtime can safely run all concurrently. |

## Observations

- server install/add writes MCP settings fragments; it does not download packages or invoke upstream tools during registration.
- npx-based presets defer package fetch/cache behavior until the command is later executed by server test, runtime, or a client.
- Remote URL domains are upstream domains, not owned by MCPace unless the user controls that endpoint. MCPace serve.publicUrl is the advertised MCPace endpoint and must point to a user-controlled relay/domain when set.
- 100 configured servers is a config-scale scenario; it does not prove safe concurrent runtime launch of 100 expensive servers.

## Warnings

- This smoke suite verifies registration semantics, not real package install latency, provider billing behavior, or live MCP tool calls.
- Run live server tests only against reviewed servers with explicit credentials and cost limits.

