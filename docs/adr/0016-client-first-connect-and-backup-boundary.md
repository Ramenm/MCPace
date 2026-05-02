# ADR 0016 — client-first connect guide and install-backup boundary

## Status

Accepted for v0.5.5.

## Context

From a user/client perspective, the most important question is not which internal module owns a feature; it is: “What do I do next to get one MCP server callable through one MCPace endpoint?” Before this pass, native server lifecycle commands existed, but the top-level README and source-quality boundaries did not fully encode that client-first path.

The client action root also still owned install backup/restore helpers alongside plan/export/install orchestration. Backup/restore is mutation support logic, not the user-facing action dispatcher itself.

## Decision

Keep `mcpace connect` as the read-only top-down guide. It resolves the MCPace endpoint, selected client target, upstream source inventory, readiness blockers, and exact follow-up commands. The first working path is documented as:

```bash
mcpace connect
mcpace server import --from ./existing-mcp-settings.json --dry-run
mcpace server add <name> --command <cmd> [--arg <arg>...]
mcpace server test <name> --refresh --json
mcpace client export <client> --json
mcpace client install <client> --dry-run
```

Extract client install backup/restore helpers into `src/client/actions/backup.rs`; keep `src/client/actions.rs` focused on action orchestration. Add a source-quality contract so the split cannot silently drift back.

Also enforce clean source archives by verifying that generated release archives exclude `.git`, `node_modules`, `target`, `dist`, temporary trees, and nested compressed artifacts.

## Consequences

- Users get one native, read-only orientation command before mutating settings or client configs.
- Existing MCP configs can be migrated with `server import`; new stdio servers can be added with `server add`; configured upstreams can be smoked with `server test` before client wiring.
- Install backup logic is easier to inspect independently from client action dispatch.
- Runtime proof is still required separately: this ADR does not claim durable HTTP sessions or remote Streamable HTTP upstream forwarding.

## Verification

- `cargo fmt --all -- --check`
- `node --test tests/node/source-quality-contract.test.js`
- `node --test tests/node/configurable-mcp-connectivity-contract.test.js`
- `node scripts/audit-source.mjs --json --write reports/source-audit-latest.json`
- `node scripts/verify-npm-pack.mjs --json`
