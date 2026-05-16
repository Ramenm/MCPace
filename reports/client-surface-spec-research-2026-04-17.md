# Client surface spec research

Date: 2026-04-17
Version: 0.6.2

This report is a packaged evidence pointer for the runtime capability fixture that tracks surface-aware client catalog work.

## Confirmed source evidence

- `src/client_catalog.rs` owns the built-in client target catalog and proof tiers.
- `docs/client-surface-matrix.md` records the local/cloud/API connector surface split.
- `docs/client-metadata-routing.md` records client, session, project-root, and metadata routing rules.

## Current conclusion

MCPace should not infer compatibility from a product or brand name alone. The catalog must distinguish local MCP clients, cloud/client-hosted surfaces, and API connector surfaces because each category has different transport, authentication, installation, and proof requirements.

## Release boundary

This evidence supports catalog/planner claims only. It does not prove live runtime success for every client surface; real-client traces remain required before broader compatibility claims.
