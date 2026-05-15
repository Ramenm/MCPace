# Tech Context

- Rust package: `mcpace`, version `0.6.0`, edition `2021`, license `Apache-2.0`.
- Node workspace package: `mcpace-workspace`, version `0.6.0`, private workspace.
- Current source checks use Node contract tests, `cargo fmt`, and source audit.
- Full Cargo check/test/build require crates.io dependency access or a populated Cargo cache.
- Streamable HTTP and stdio MCP are the relevant transports; remote HTTP upstream forwarding is not yet implemented as callable fan-out.

## v0.5.6 notes

- `connect` modules: `src/connect.rs`, `src/connect/args.rs`, `src/connect/model.rs`, `src/connect/render.rs`.
- MCP source import/toggle modules: `src/mcp_sources/import.rs`, `src/server/import.rs`, `src/server/toggle.rs` plus write/toggle helpers in `src/mcp_sources/write.rs`.
- Source audit latest: `critical: []`, `warnings: []`, `largeModules: 0`, `productionUnwraps: 0`.

## v0.5.6 connect technical notes

- `src/connect.rs` is a read-only orchestration command.
- `src/connect/model.rs` composes `runtimepaths`, `mcp_sources`, `server` inventory, `client_catalog`, and `verify`.
- `mcpace.connectReport.v1` is the current JSON schema marker for connect reports.
- The command intentionally avoids MCP settings writes/removes/toggles and direct filesystem mutation.


## v0.5.9 notes

- Preset catalog loader: `src/mcp_presets.rs`.
- Packaged preset data: `presets/mcp-servers.json`.
- Preset rendering: `src/server/preset_render.rs`.
- Config/schema surface: `mcpace.config.json` and `schemas/mcpace-config.schema.json` `mcpPresets.includePaths`.
- Cargo exists in this environment, but `cargo check --all-targets --locked` is blocked by crates.io DNS/dependency access.

## v0.5.9 install/readiness technical notes

- Inventory scripts: `scripts/inventory-source.mjs`, `scripts/inventory-project.mjs`.
- Harness scripts: `scripts/boot-harness.mjs`, `scripts/install-readiness-harness.mjs`.
- Current boot harness schema markers: `mcpace.sourceInventory.v1`, `mcpace.codeInventory.v2`, `mcpace.bootHarness.v1`, `mcpace.installReadiness.v1`.
- Current npm package verification mode: `thin-launcher` until native platform binaries are staged.
- Current environment proof: `cargo fmt` passes; `cargo check/test/build` are blocked by crates.io DNS/dependency access.
