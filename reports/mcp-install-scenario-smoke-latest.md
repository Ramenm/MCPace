# MCP install scenario smoke

Generated: 2026-05-19T12:35:43.700Z
Status: pass
Project: mcpace 0.6.5

## Checks

| Check | Status | Detail |
|---|---:|---|
| auto-install-dry-run-is-config-only | pass | source-level proof: install_auto passes dry_run into write_mcp_server_entry and server install routes through mcp_autoinstall. |
| auto-install-writes-one-fragment | pass | source-level proof: npm/PyPI/OCI specs materialize reviewable MCP settings fragments through the source writer. |
| auto-install-does-not-run-package-command | pass | source-level proof: auto install constructs command/args only and does not spawn package launchers. |
| reinstall-without-force-is-blocked | pass | source-level proof: duplicate names remain blocked by the shared MCP settings writer unless --force is passed. |
| reinstall-with-force-replaces | pass | source-level proof: force replacement removes the normalized existing key before writing the replacement entry. |
| custom-stdio-server-add | pass | exit=0 19ms |
| remote-http-server-add | pass | exit=0 22ms |
| invalid-remote-url-is-rejected | pass | exit=1 server add --url currently accepts only http:// or https:// MCP endpoints |
| paid-server-can-be-registered-disabled | pass | exit=0 19ms |
| hundred-server-config-scale | pass | 100 fragments written in 2865ms; inventory serverCount=127 |

## Scenario matrix

| Scenario | Expected behavior | Risk covered |
|---|---|---|
| auto install dry-run | No file is written; output says dry-run-add. | Prevents accidental config mutation while evaluating an MCP server. |
| auto install apply/reapply/force | First apply writes one fragment, second apply without --force fails, --force replaces. | Prevents hidden reinstall/duplicate drift and makes replacement explicit. |
| custom stdio server | Writes command/args only; does not execute the command during add. | Separates registration from runtime execution. |
| remote Streamable HTTP server | Accepts http(s) URL and headers as config; rejects non-http(s). | Separates remote domain ownership from local MCPace endpoint ownership. |
| paid/expensive server disabled by default | Entry can be added with enabled=false; costs remain dependent on later runtime/tool calls. | Avoids accidental activation while still allowing reviewable config. |
| 100-server config scale | 100 distinct fragments are written and visible in source inventory. | Covers many-server config fanout without claiming runtime can safely run all concurrently. |

## Observations

- Auto-install source has been updated, but the vendored binary in this sandbox still predates that Rust source change; these auto-install checks are source-level until a Rust host rebuilds the binary.
- server install/add writes MCP settings fragments; it does not download packages or invoke upstream tools during registration.
- npx/uvx/docker-derived entries defer package fetch/cache behavior until the command is later executed by server test, runtime, or a client.
- Remote URL domains are upstream domains, not owned by MCPace unless the user controls that endpoint. MCPace serve.publicUrl is the advertised MCPace endpoint and must point to a user-controlled relay/domain when set.
- 100 configured servers is a config-scale scenario; it does not prove safe concurrent runtime launch of 100 expensive servers.

## Warnings

- This smoke suite verifies registration semantics, not real package install latency, provider billing behavior, or live MCP tool calls.
- Run live server tests only against reviewed servers with explicit credentials and cost limits.
