# ADR 0017 — Preset-first useful MCP installs without Rust package hardcode

Status: Accepted for v0.5.6.

## Context

The BYO MCP workflow had native commands for adding, importing, testing, enabling,
disabling, and removing servers. It was still too hard for first-time users because
installing a useful server required remembering package names and argument order.
Putting a growing list of third-party server package names into Rust code would make
MCPace harder to maintain and would confuse product truth with external catalog
curation.

## Decision

Add an editable preset catalog at `presets/mcp-servers.json` and expose it through:

```bash
mcpace server presets
mcpace server install <preset> [--path <path>...] [--dry-run]
mcpace server starter [--path <path>...] [--dry-run]
```

The Rust source loads preset data, validates it, expands path arguments where a
preset declares `pathMode: append`, and delegates the actual settings write to the
existing `mcp_sources::write_mcp_server_entry` path.

## Consequences

- Common useful installs become one short command.
- The default starter pack can remain conservative and safe.
- Third-party package names live in data, not in Rust logic.
- Source/release contracts now include `presets/`.
- Remote registry integration remains a separate future discovery/import lane.

## Not goals

- Do not silently run or install every preset.
- Do not claim remote HTTP upstream forwarding is implemented.
- Do not turn MCPace into an opinionated, hardcoded MCP marketplace.
