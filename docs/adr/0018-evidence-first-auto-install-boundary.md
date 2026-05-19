# ADR 0018 — Superseded by evidence-first auto install

Status: superseded.

The current implementation keeps useful MCP onboarding out of a bundled catalog. `src/mcp_autoinstall.rs` derives install plans from package, URL, OCI, or local command specs. `src/server/install.rs` is a thin command wrapper, while `src/server/render.rs` renders the resulting install/write report together with the rest of the server command surface.

The source profiler then uses transport, launcher, command/url, args, optional operator policy, and live probe evidence to choose conservative scheduling boundaries. This keeps the behavior understandable without a static packaged server-family catalog.
