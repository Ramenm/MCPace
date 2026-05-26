# Security Policy

## Reporting a vulnerability

Please report security issues privately instead of opening a public issue.

Use GitHub private vulnerability reporting when it is available for this repository. If it is not available, contact the repository owner directly with:

- affected version or commit;
- operating system and architecture;
- reproduction steps;
- expected and actual impact;
- whether local files, credentials, environment variables, MCP tool permissions, or upstream MCP servers are involved;
- whether the issue requires local access, a malicious upstream server, a malicious client, or network exposure.

Please do not include live credentials, tokens, private keys, or private file contents in reports. Redacted logs are preferred.

## Supported versions

Until the first public stable release, only the current `main` branch and the latest tagged pre-release are in scope for security fixes.

| Version | Security support |
|---|---|
| Latest `main` | Yes |
| Latest tagged pre-release | Best effort |
| Older commits/tags | No, unless the maintainer explicitly says otherwise |

## Current security boundary

MCPace is a local MCP hub. Treat configured upstream MCP servers as trusted local extensions unless their policy explicitly says otherwise. Do not enable upstream servers or tool-risk allow flags for workflows you do not trust.

User-specific MCP server configuration belongs outside the repository, for example in a user-owned file referenced by `MCPACE_MCP_SETTINGS`.

## Localhost and network exposure

The default product posture is local-first. Binding to `127.0.0.1` is different from binding to `0.0.0.0` or exposing MCPace through a tunnel, relay, or public URL.

Before any public/non-local mode is treated as supported, MCPace should require explicit security configuration such as authentication, clear origin policy, and user-visible warnings. Origin checks are useful defense-in-depth, but they are not a replacement for authentication on a public endpoint.

## Upstream MCP servers

MCPace does not silently install arbitrary upstream MCP servers. Presets write reviewable config fragments, and runtime execution still happens through commands or URLs the user configured.

Security-sensitive upstream concerns include:

- filesystem scope;
- command execution;
- inherited environment variables;
- bearer/API tokens;
- network reachability;
- tool-risk allow flags;
- malicious or compromised upstream packages.

## Dynamic discovery and install safety

`mcpace auto` may search local catalogs and a cached/refreshable MCP Registry response, but auto mode is trust-gated. MCPace must not silently execute a random public MCP server package just because it matched a query.

Default install rules:

- `mcpace auto` refreshes stale registry metadata, then installs only `trusted` or `approved` candidates;
- `review` matches require a local catalog/config trust decision or an advanced review flag;
- unknown, blocked, deprecated, deleted, or ambiguous candidates stay plan-only;
- after any trusted install, auto mode runs the same live `initialize`/`tools/list` probe path used by `mcpace server test <name> --refresh` unless run with `--dry-run`.

## Logging and diagnostics

Diagnostics should remain bounded and should redact likely secrets such as tokens, API keys, passwords, bearer values, private keys, and authorization headers. If you find unredacted secret material in logs, reports, dashboard output, or CLI diagnostics, report it privately.

## Public issues

Public issues are appropriate for general hardening requests, documentation gaps, and non-sensitive bugs. Do not use public issues for exploitable vulnerabilities or leaks of private data.
