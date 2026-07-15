# Holistic runtime model and product invariants

This document describes how MCPace is expected to work end to end, without assuming that install, settings, server discovery, runtime startup, and release packaging are independent systems.

## End-to-end flow

1. The npm launcher resolves the native binary for the current platform. Native package metadata, target identity, executable bit, version, and realpath containment are checked before execution.
2. The native CLI loads project/runtime roots and private state directories. Runtime, cache, serve, hub, and bin directories are private on Unix and reject symlink/non-directory replacements.
3. MCP server definitions are loaded from configured settings sources. Sources are regular JSON files; directory discovery skips symlinked directories and non-regular entries.
4. Server write operations take a global MCP settings namespace lock and deterministic per-file locks before read-modify-write updates. This prevents cross-source duplicate shadowing and lost updates across concurrent `server add/remove/enable/import` operations.
5. Discovery and registry cache refresh use normalized HTTPS endpoints, exclusive locks, content-based cache keys, private atomic writes, and deterministic candidate precedence.
6. Auto-install planning validates package identifiers as data, not shell snippets. Registry entries cannot smuggle local paths, options, whitespace/control characters, URLs in package positions, or shell-composition tokens.
7. `serve start` owns a serve-state lock while deciding whether a runner is already active, then binds the built-in HTTP server to loopback. The built-in server does not terminate TLS, so direct non-loopback bind flags are rejected; remote access requires a trusted HTTPS reverse proxy or tunnel on the same host.
8. Streamable HTTP requests are parsed with size limits, single Host-header enforcement, same-authority Origin validation, ASCII header-value validation, content-type/accept checks, session gating, and JSON-RPC lifecycle enforcement. Session touch, request-id replay tracking, and initialized-state classification are performed under one session lock so parallel HTTP requests cannot interleave those decisions. Replay IDs are bounded per session and by a 16 MiB process-wide store budget.
9. Upstream stdio servers are spawned only when the user actually connects/invokes server-backed operations. Tool-list cache warmup is off by default and requires explicit `MCPACE_TOOL_LIST_WARMUP=1|true|yes|on|enabled` opt-in so that serving the UI does not silently execute local MCP startup commands.
10. Release artifacts are built from a Git-tracked-file allowlist plus a normalized manifest. Real builds reject untracked or ignored inputs, symlinks, non-regular files, and portable path collisions; ZIP entries use stable timestamps and receive a manifest/verification report.

## System invariants

- One normalized MCP server name resolves to one authoritative source unless the user explicitly chooses a source to edit.
- No local MCP server startup command is run as a side effect of merely opening the dashboard or starting the local UI.
- HTTP transport stays local-first: the built-in listener, Host, and Origin are loopback-only; token support remains available; remote plain-HTTP upstreams are rejected unless loopback. A trusted HTTPS reverse proxy or tunnel may expose the loopback listener, but MCPace never advertises direct cleartext non-loopback bearer authentication.
- JSON-RPC lifecycle is stateful: `initialize` happens before normal requests; initialized notification is recognized; HTTP notifications/responses return `202 Accepted` when accepted.
- File updates that affect user settings or release evidence are private, locked where shared, and atomic where visible to other processes.
- npm/native package resolution and release publishing fail closed when target metadata, provenance, or native target artifacts are missing.

## Remaining release gates

The Node/npm/release layer can be tested locally. Final publication still needs:

- real Rust toolchain run: fmt, clippy, tests, native build, native smoke;
- six publishable native target packages/tarballs;
- CI-side registry signature/provenance checks with network access;
- repository-setting proof for protected release environments, immutable tags, and trusted npm publishers.

## Whole-system edge cases rechecked

- A JSON-RPC request that arrives before `notifications/initialized` is still treated as a handled request for replay-detection purposes, but touch, request-id tracking, and readiness classification now happen while holding the same HTTP session lock.
- Importing MCP settings rejects symlink and non-regular source files before parsing, matching the target-side regular-file policy.
- The built-in HTTP server is loopback-only because it does not terminate TLS. Deprecated non-loopback flags fail with guidance to use a trusted HTTPS reverse proxy or tunnel.
- Host and Origin must name the same exact loopback authority, including the port, so cross-localhost origins, `localhost.evil.example`, userinfo-style authorities, control characters, and malformed ports stay rejected.
