# Project brief

## Confirmed from repository

- Project name: `mcpace`.
- Workspace/package version: `0.5.9`.
- Current product shape: Rust-first local MCP hub/control plane.
- Public entrypoint contract: `serve` is the product; `hub` is internal/operator-facing lifecycle machinery; `dashboard` is an optional view/control surface.
- Packaged upstream MCP defaults are intentionally empty: `mcp_settings.json.mcpServers` is `{}`, `mcpace.config.json.servers` is `{}`, and `server-candidates.json` is `[]`.
- Primary current promise is captured in `docs/product-truth.json`: one local MCPace endpoint with generic MCP brokering/diagnostics and no bundled upstream MCP servers enabled or recommended by default.

## Real task

Prove one production-like loop before broad claims:

`real local MCP client -> mcpace serve -> /mcp -> initialize -> notifications/initialized -> tools/list -> tools/call -> user-configured upstream stdio MCP server -> response -> diagnostics`

## Non-goals for the current cycle

- Do not claim a public cloud relay as a supported product lane.
- Do not claim broad universal runtime support before proof-tier hosts are verified.
- Do not enable upstream MCP servers by default. Useful MCP preset catalogs may exist as explicit, reviewable install recipes, but they must not silently install or run servers.
- Do not treat source/archive proof as release, runtime, or publish proof.

## НЕ ПОДТВЕРЖДЕНО

- Current archive does not include a built Rust binary.
- Full Rust build/test status is not confirmed in this sandbox because Cargo dependency resolution is blocked by crates.io DNS/network access.
- Real-host traces for all tier-1 clients are not present in the clean archive.
