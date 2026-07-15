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

Until the first public stable release, only the current `main` branch and the latest tagged release are in scope for security fixes. A stable-looking pre-1.0 tag such as `v0.x.y` is still the tagged release for this policy; GitHub's optional prerelease label does not change support.

| Version | Security support |
| --- | --- |
| Latest `main` | Yes |
| Latest tagged release | Best effort |
| Older commits/tags | No, unless the maintainer explicitly says otherwise |

## Current security boundary

MCPace is a local MCP hub. Treat configured upstream MCP servers as trusted local extensions unless their policy explicitly says otherwise. Do not enable upstream servers or tool-risk allow flags for workflows you do not trust.

User-specific MCP server configuration belongs outside the repository, for example in a user-owned file referenced by `MCPACE_MCP_SETTINGS`.

## Localhost and network exposure

The built-in HTTP server is loopback-only because it does not terminate TLS. It rejects `0.0.0.0`, LAN/public addresses, and the deprecated non-loopback opt-in flags even when a bearer token is configured; reusable bearer credentials must not be sent over direct cleartext non-loopback HTTP.

For remote access, terminate HTTPS in a trusted reverse proxy or tunnel on the same host and forward only to MCPace's loopback listener. The proxy must authenticate users, restrict origins, and rewrite both upstream Host and Origin to the same loopback authority (including port) expected by MCPace. Configure `MCPACE_HTTP_AUTH_TOKEN` for every proxied deployment as defense in depth; invalid credentials are rate-limited before request bodies are read. Rotate the token if proxy or network logs could have captured it. Origin checks are not a replacement for proxy authentication.

## Upstream MCP servers

MCPace does not silently install arbitrary upstream MCP servers. Presets write reviewable config fragments, and runtime execution still happens through commands or URLs the user configured.

Remote Streamable HTTP upstreams use HTTPS with operating-system certificate verification. MCPace does not follow upstream redirects, validates configured header names/values, prevents overriding transport-owned headers, and never copies Registry credential placeholders into configuration. Plain HTTP upstreams are accepted only for `localhost` or parsed loopback IP addresses.

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

- a no-query `mcpace auto` uses only the pinned embedded/local curated catalog; named searches refresh a bounded query-specific Registry cache;
- Registry publication and publisher-supplied fields never grant trust: Registry entries remain `review` even when a custom cache path is configured;
- `review` matches require a local catalog/config trust decision or an advanced review flag;
- unknown package managers, custom package registry bases, blocked/deleted entries, repeated cursors, malformed metadata, and ambiguous matches stay plan-only or fail closed;
- required Registry arguments, environment values, URL variables, and HTTP headers must be supplied explicitly; credential placeholders are not persisted as secrets;
- after any trusted install, auto mode runs the same bounded, paginated `initialize`/`tools/list` probe path used by `mcpace server test <name> --refresh` unless run with `--dry-run`, and failed probes are not reported as ready.

## Logging and diagnostics

Diagnostics should remain bounded and should redact likely secrets such as tokens, API keys, passwords, bearer values, private keys, and authorization headers. If you find unredacted secret material in logs, reports, dashboard output, or CLI diagnostics, report it privately.

## Public issues

Public issues are appropriate for general hardening requests, documentation gaps, and non-sensitive bugs. Do not use public issues for exploitable vulnerabilities or leaks of private data.
