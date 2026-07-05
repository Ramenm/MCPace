# MCPace release/source summary

This source bundle is intended to be reviewable without carrying machine-local runtime state.

Validation highlights:

- `npm pack` is verified through the npm publish contract tests.
- Python package evidence uses `pip download --no-deps` for metadata collection only.
- The lab harness is not executing foreign MCP server code; random and unknown servers stay in plan-only or metadata-only audit paths until explicitly approved.
- The random held-out audit records package metadata, evidence layers, and decision traces without shipping sandbox download artifacts.
- Clean archive policy rejects local runtime files such as `data/runtime/mcpace.sqlite`, generated `mcpace-autostart.vbs`, tool-list caches, logs, and server-state files.

The source bundle should include docs, tests, schemas, manifests, scripts, and review reports. It should not include local state, cached upstream downloads, generated runtime binaries, or machine-specific autostart scripts.
