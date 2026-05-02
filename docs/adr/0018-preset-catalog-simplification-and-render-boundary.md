# ADR 0018 — Preset catalog simplification and render boundary

Status: Accepted for v0.5.9.

## Context

First-run users need useful MCP servers without memorizing package names or hand-editing JSON. The v0.5.6 preset flow solved the basic install problem, but it still treated the packaged preset catalog as the practical source and kept preset rendering inside the generic server renderer. That made future useful-MCP growth feel less native than the rest of the BYO MCP lifecycle.

## Decision

Keep third-party MCP package definitions in data, not Rust code, and make the preset data source extensible:

- load preset catalogs from `mcpace.config.json` `mcpPresets.includePaths`;
- fall back to `presets/mcp-servers.json` when no config path is present;
- extend/override with `MCPACE_MCP_PRESETS`;
- report merged catalog sources and warnings through `mcpace server presets --json`;
- keep install-time overrides via `--arg` and `--env`;
- support a repository-scoped path mode for git-style presets;
- move preset output to `src/server/preset_render.rs` instead of growing `src/server/render.rs`.

## Consequences

- Common useful installs remain short commands.
- Teams can add their own preset catalog without recompiling MCPace.
- The packaged catalog can stay small and reviewable.
- Generic server rendering remains simpler.
- The default starter stays conservative: filesystem only. Network docs, git repository context, and browser automation remain explicit opt-ins.

## Risks

- Preset catalogs are still command execution recipes. Users should review preset data and use `--dry-run` before writes.
- Remote HTTP upstream entries are still inventory-only until the remote connector is implemented.
- Preset installs are convenience wrappers; real runtime proof still requires `server test` and a real client trace.

## Follow-up

- Add `mcpace server test <name>` examples for each packaged preset once the Rust build lane is green.
- Consider registry-backed discovery/import as a separate lane using the official MCP Registry metadata API, not as Rust hardcode.
