# Legacy cleanup pass

This pass checked for leftover retired bridge surfaces after the prior consolidation passes.

## Removed

- Removed `manager.settings.json` from the source tree and release manifest.
- Removed `manager.settings.json` documentation from `docs/configuration.md`.
- Removed runtime-profile selection from the retired manager settings bridge. Runtime profile selection now uses `MCPACE_RUNTIME_PROFILE` first, then `mcpace.config.json`.
- Removed `compatibility.legacyManagerBridge` and `compatibility.legacyScriptAliases` from bundled hub examples and `schemas/mcpace-hub.schema.json`.
- Removed the disabled projected-tool top-level-control bridge guarded by `MCPACE_PROJECTED_LEGACY_TOP_LEVEL_CONTROLS`; projected adapter controls now live under `_mcpace` or `mcpace` only.
- Removed the unused `shape_upstream_structured_content` helper.
- Renamed the internal old-schema tool surface helper from `legacy` to `compact`.

## Intentionally kept

- `sse-legacy`, `legacy-compat`, `PX_legacy_compat`, and `legacy-disabled` remain because they describe blocked or downgraded HTTP+SSE transports in server/profile schemas and runtime diagnostics. They are not retired bridge code.
- `compat` result mode remains as an explicit serialized JSON output mode. It no longer carries legacy wording in public tool descriptions.

## Verification

- `npm run check`
- `npm run pack:npm:dry-run`
- `npm run release:dry-run`
