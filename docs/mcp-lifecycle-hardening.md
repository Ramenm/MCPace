# MCP lifecycle and server-source hardening

This pass documents the invariants MCPace enforces around MCP server discovery, installation planning, settings writes, and local HTTP/stdio boundaries.

## Settings source writes

MCP settings changes are read-modify-write operations. `server add`, `server remove`, `server enable`, `server disable`, and `server import` therefore acquire an exclusive per-settings-file lock before reading and writing the JSON source. Writes use a private atomic write path so other processes do not observe a partially written JSON file and so newly created settings files are not world-readable on Unix.

Settings sources must be regular files. Symlink sources, non-regular files, and symlinked settings directories are rejected or skipped with a warning. Directory sources only load regular `*.json` entries.

## Duplicate and shadowed server names

Server identity is based on the normalized MCP server name. MCPace now checks all configured settings sources before adding or importing a server entry. A name that already exists in another source is treated as a shadowing conflict instead of silently creating an ambiguous definition.

When removing or toggling a server without `--settings`, MCPace refuses ambiguous matches across multiple sources and asks the caller to choose the exact settings file.

## Cache invalidation

The upstream server cache fingerprint is content-based. It includes source paths, settings bytes, unreadable/missing-source markers, and collection warnings. This avoids timestamp/length collisions where a settings change can be missed if two edits happen in the same timestamp granularity window.

## Discovery and registry refresh

Registry endpoints are normalized before use. Refresh endpoints must be HTTPS URLs without credentials, fragments, whitespace, or control characters. Registry cache refresh uses an exclusive lock and private atomic write.

Discovery candidates are deduplicated by normalized server name. Installed candidates, higher trust levels, approved catalogs, and higher scores take precedence over lower-trust duplicates.

Registry fetching uses trusted absolute fetch-tool paths where available, a cleaned environment, and `--` before the fetched URL for curl. PowerShell fallback passes the URL as a parameter rather than interpolating it into a command string.

## Install planning

Direct `npm:`, `pypi:`, `uvx:`, `oci:`, and command-like install specs are validated as single registry/package/image identifiers. Auto-install planning rejects whitespace/control characters, leading option-like values, shell-composition tokens, URLs in package positions, and local path/alias forms for non-OCI package specs.

## Enabled/disabled compatibility

Imported MCP settings often use either `enabled: false` or `disabled: true`. MCPace now treats `disabled: true` as authoritative disabled state, then falls back to `enabled` with a default of enabled.

## Plain HTTP boundary

Direct plain-HTTP upstream URLs are restricted to loopback hosts. Remote MCP endpoints should use TLS, a local gateway, or stdio.

## Local server startup consent boundary

Serving the dashboard must not silently execute configured local MCP server commands. The tool-list cache warmup is therefore opt-in: set `MCPACE_TOOL_LIST_WARMUP=1`, `true`, `yes`, `on`, or `enabled` to allow background `tools/list` warmup. When the variable is unset or set to any other value, warmup is disabled and upstream stdio commands start only from explicit connection or invocation flows.
