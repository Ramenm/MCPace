# Code inventory — 2026-05-02 / v0.5.5

Generated after the dashboard/source-registry/upstream/adapter/client-action modularization pass.

## Counts

- `totalFiles`: 396
- `rustFiles`: 111
- `nodeFiles`: 69
- `markdownFiles`: 96
- `jsonFiles`: 84
- `docsFiles`: 61
- `reportFiles`: 37
- `schemaFiles`: 2

## Largest Rust modules after modularization

| file | lines |
|---|---:|
| `tests/client_surface.rs` | 1628 |
| `src/client/actions.rs` | 1453 |
| `src/adapter.rs` | 1452 |
| `src/client_catalog.rs` | 1311 |
| `src/mcp_server.rs` | 1310 |
| `src/hub/leases.rs` | 1310 |
| `src/adapter/discovery.rs` | 1279 |
| `src/upstream/tests.rs` | 1277 |
| `src/serve.rs` | 1229 |
| `src/mcp_server/tool_surface.rs` | 1212 |
| `tests/mcp_server.rs` | 1095 |
| `src/dashboard/tests.rs` | 1027 |
| `src/dashboard/http_tools.rs` | 996 |
| `src/upstream/lease_runtime.rs` | 988 |
| `src/dashboard.rs` | 965 |
| `src/upstream.rs` | 934 |
| `src/service.rs` | 744 |
| `src/setup.rs` | 693 |
| `src/tool_result.rs` | 689 |
| `tests/hub_leases.rs` | 681 |
| `src/client/plan.rs` | 655 |
| `src/doctor.rs` | 640 |
| `src/lab/render.rs` | 640 |
| `src/client/actions/render_models.rs` | 607 |
| `tests/config_and_server.rs` | 568 |

## Notes

- No production Rust module currently exceeds the source-audit large-module threshold of 1500 lines.
- `src/dashboard.rs`, `src/upstream.rs`, `src/adapter.rs`, and `src/client/actions.rs` are now below the threshold after extracting focused HTTP, upstream runtime, discovery, render-model, config-update, cache, lease/session, source-type, diagnostics, projection, and test modules.
- The remaining large files are either tests or production modules still below the audit threshold; further refactoring should be behavior-driven and gated by Cargo check/test/build.
