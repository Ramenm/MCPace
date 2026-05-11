# System patterns

## Observed architecture

- `serve` is the public product entrypoint.
- `hub` is internal/operator-facing lifecycle machinery.
- `dashboard` is an optional HTTP/UI/control surface.
- User upstream MCP servers are BYO: packaged defaults stay empty; runtime config is merged from root `mcp_settings.json`, `mcp_settings.d/*.json`, `mcpace.config.json` include paths/dirs, and MCPACE_* settings environment sources.
- `mcpace.config.json` server entries are policy/routing metadata, not a required bundled server catalog.
- Client-surface support is catalog-driven from `src/client_catalog.rs`, with proof focus selected by `proofTier = tier-1`.

## Transport and runtime patterns

- Local HTTP MCP endpoint is `/mcp`.
- Stdio upstream children are launched as subprocesses.
- The upstream child environment is cleared before launch, then rebuilt from a small launch baseline plus explicit `env` values and allowlisted local `env_vars` values.
- Upstream stderr diagnostics are bounded and sanitized before surfacing errors.
- Cache/session fingerprints should not embed plaintext secret values.

## Safety and release patterns

- Empty upstream defaults are intentional.
- Source proof, runtime proof, release proof, and publish proof are separate gates.
- Reports must not claim proof that was not executed on the relevant host/toolchain.
- The source archive contract is defined by `release-manifest.json`; required paths must exist in a clean archive.

## Current hardening decision from this session

- `memory-bank/` is treated as a tracked source artifact because `release-manifest.json`, `reports/summary.md`, and `tests/node/security-contract.test.js` already treat it as required project context.
- `/mcp` route handling should reject invalid `Origin` with HTTP `403`, advertise `Allow: POST` when SSE GET is not supported, and require Streamable HTTP POST `Accept` entries for `application/json` and `text/event-stream`.

## НЕ ПОДТВЕРЖДЕНО

- Durable `Mcp-Session-Id` storage/enforcement is not implemented; compatible header minting on initialize is implemented at source level but still needs Rust/runtime proof.
- Real-host upstream tool-call traces are not confirmed in the clean archive.

## MCP HTTP and proof-env hardening patterns

- `/mcp` remains compatibility-first: older clients may omit `Mcp-Method` and `Mcp-Name`, but if they send those headers the values must agree with the JSON-RPC body.
- Header/body mismatches use JSON-RPC `ERROR_HEADER_MISMATCH = -32001` and HTTP `400 Bad Request`.
- Generic proof child processes must use `scripts/lib/safe-child-env.mjs`; they must not pass `{ ...process.env }` unless a script has an explicit publish/runtime reason.
- npm publish credentials are intentionally scoped to `scripts/publish-npm-artifacts.mjs`.
- Node source/npm tests should stay serial and forced-exit unless a later CI run proves parallel execution is deterministic.


## v0.5.6 configurable MCP ingress pass

- Added project/env-configurable advertised MCPace endpoint via `serve.*`, `ports.serve`, `MCPACE_SERVE_*`, and `MCPACE_PUBLIC_MCP_URL`.
- Added multi-source upstream registry via `src/mcp_sources.rs`, `mcpSettings.includePaths`, and `MCPACE_MCP_SETTINGS`.
- `/mcp` `initialize` now returns `Mcp-Session-Id` and `MCP-Protocol-Version`; HTTP upstream lease context recognizes extra client/chat/project-root headers.
- New source contract test: `tests/node/configurable-mcp-connectivity-contract.test.js`.
- Verified after this pass: `npm test`, `node scripts/audit-source.mjs --json`, `node scripts/proof-report.mjs --json --write`, `node scripts/build-release-artifacts.mjs --json`, and `node scripts/verify-npm-pack.mjs --json`.
- Still not verified: Rust/Cargo checks and real-client runtime trace.


## Configured endpoint routing pattern

Client-facing endpoint values must come from `runtimepaths::resolve_serve_endpoint`. Any advertised path must be accepted by the HTTP router and by setup/runtime smoke probes; defaults remain `/mcp` and `/healthz` for backwards compatibility.

## v0.5.6 inventory patterns

- Multi-source upstream configuration has one intended source of truth: `mcp_sources::load_mcp_server_registry`. Runtime routing, server inventory, and doctor/readiness should all use this registry instead of reading only root `mcp_settings.json`.
- HTTP response headers derived from client-controlled request headers must be normalized before echo. For MCP Streamable HTTP session ids, only visible ASCII values bounded by `resources::MAX_HTTP_HEADER_LINE_BYTES` are eligible for echo; otherwise MCPace generates a local id.
- Generated local HTTP session ids should prefer OS randomness where available; fallback ids must be clearly identifiable as fallback and must not be treated as durable authenticated sessions.

## v0.5.6 convenience patterns

- Keep the packaged upstream catalog empty, but provide native onboarding commands.
- Prefer one-server-per-file MCP fragments under `mcp_settings.d/` for project-local BYO MCP composition.
- Keep command-family roots thin: new `server` behaviors should live under `src/server/*.rs` rather than expanding `src/server.rs`.
- Source inventory and runtime loading must share `mcp_sources::load_mcp_server_registry` / `load_mcp_source_report` rather than reading only root `mcp_settings.json`.

## v0.5.6 patterns

- Keep BYO MCP lifecycle commands symmetrical: add/list/sources/remove should use the same multi-source registry and avoid requiring hand-edited root JSON.
- Normalize upstream transport aliases at runtime boundaries before diagnostics; do not let `streamable-http`/`remote-http` drift into separate blocked-state meanings unless the HTTP connector is implemented.
- Schema coverage should track the real `mcpace.config.json` surface (`serve`, `mcpSettings`, `clientCatalog`) rather than only the older hub example schema.

## v0.5.6 module split patterns

- Keep command-family roots thin. New `server` subcommands should live under `src/server/*.rs` and reuse shared registry/probe code instead of growing `src/server.rs`.
- Keep HTTP route roots as orchestration layers. Dashboard boundary/header/session/tool/diagnostic/response helpers should remain under `src/dashboard/` child modules.
- Extracted `src/**/tests.rs` files are test modules and should not be counted as production Rust debt.
- Source audit now reports zero production large-module warnings; do not continue splitting purely for line count. Prefer behavior-preserving child modules and source contracts, especially until Cargo check/test/build is available.
- BYO MCP onboarding should be symmetric and native: add -> sources/list -> test -> client install/export -> remove.

## v0.5.6 module split and native smoke pattern

- Keep command roots thin (`src/server.rs`, `src/dashboard.rs`, `src/upstream.rs`) and move cohesive implementation into sibling child modules.
- Prefer native CLI lifecycle over manual JSON editing for user-facing MCP server management:
  - add/update server fragments;
  - list source paths;
  - smoke-test one configured upstream;
  - remove a server from the source where it was found.
- Do not hardcode public client endpoints in feature code. Use `runtimepaths::resolve_serve_endpoint` or `runtimepaths::public_mcp_url_or_placeholder` and keep placeholders centralized.
- Keep remote HTTP MCP entries inventory-only until an explicit HTTP upstream connector with auth isolation and SSRF controls exists.


## Dashboard MCP HTTP boundary split

- Keep `src/dashboard.rs` as listener/route orchestration.
- Keep MCP HTTP route dispatch in `src/dashboard/mcp_http.rs`.
- Keep Origin/Accept, MCP standard headers, session ids, tool definitions, and HTTP tool runtime in focused child modules.
- Do not reintroduce root-only `mcp_settings.json` wording for runtime behavior that uses the merged MCP settings registry.

## v0.5.6 adapter split pattern

When splitting adapter behavior, keep the root file as the type/options/projection orchestrator and move focused rendering/encoding behavior into child modules. Child helpers that the root legitimately consumes should be marked `pub(super)` rather than left private or widened to `pub` unnecessarily.


## v0.5.6 catalog/stdio boundary patterns

- Keep built-in client defaults in `src/client_catalog/builtin.rs`. The root `src/client_catalog.rs` should own types, external registry loading, merge behavior, and selectors.
- Keep stdio MCP argv parsing/help in `src/mcp_server/args.rs`. The root `src/mcp_server.rs` should stay focused on JSON-RPC/MCP lifecycle, tool dispatch, and command bridge behavior.
- When source tooling needs static client defaults, read `src/client_catalog/builtin.rs` first and retain old-location fallback only for transition compatibility.

## v0.5.6 import and client-action boundary pattern

User-supplied MCP servers can be created one-by-one (`server add`) or migrated from an existing `mcpServers` JSON config (`server import`). Import preserves the original server entry shape and uses conflict detection by normalized server name. `--dry-run` should be used before writes; `--force` is required for replacement. Do not add a hardcoded upstream catalog as a shortcut.

Read-only client catalog listing lives in `src/client/actions/list.rs`; install/export/restore mutation paths stay in `src/client/actions.rs` and child mutation helpers. Keep new client actions split by user-facing responsibility.


## v0.5.6 client-first connect and backup patterns

- Treat `mcpace connect` as the read-only user orientation surface; it should not mutate MCP settings or client config files.
- Keep the first working path explicit: connect -> import/add -> server test -> client export/install.
- Keep native BYO MCP commands symmetric and reversible where possible: import/add, sources/list/capabilities, test, enable/disable, remove.
- Keep client action mutation support in child modules. Install backup/restore helpers belong in `src/client/actions/backup.rs`; read-only listing belongs in `src/client/actions/list.rs`; the root should orchestrate rather than accumulate helpers.
- Clean release archives must exclude `.git`, dependency directories, build outputs, temp trees, and nested compressed artifacts.

## v0.5.6 client-first pattern

User-facing onboarding should start with read-only inspection before mutation. `mcpace connect` resolves endpoint/client/upstream/readiness state and returns commands; `server import/add/enable/disable/remove` mutate only explicit MCP settings sources and support dry-run where practical. Do not add hardcoded upstream MCP catalogs; keep BYO server state in `mcp_settings.json`, `mcp_settings.d/*.json`, configured include paths/dirs, or explicit environment sources.

## v0.5.6 client-first connect pattern

A user-facing onboarding command should compose existing sources of truth instead of duplicating them. `mcpace connect` is read-only and resolves endpoint, upstream source inventory, server records, client target, readiness blockers, and exact next commands through existing modules. Keep it free of MCP settings/client-config mutation helpers; mutation remains in `server add/import/enable/disable/remove` and `client install`.


## v0.5.9 preset simplification pattern

- Useful MCP package recipes must live in preset catalog data, not Rust package-name literals.
- Load preset catalogs from `mcpPresets.includePaths`, fallback `presets/mcp-servers.json`, and `MCPACE_MCP_PRESETS`; report sources/warnings through `server presets --json`.
- Keep starter packs conservative; network docs, repository context, and browser automation remain explicit opt-ins.
- Keep preset rendering in `src/server/preset_render.rs` and generic server rendering in `src/server/render.rs`.
## v0.5.9 server thin-root pattern

- Keep `src/server.rs` as a dispatcher under the thin-module-root contract.
- Put configured-server list/capability query behavior in `src/server/query.rs`; keep generic rendering in `src/server/render.rs` and preset-specific rendering in `src/server/preset_render.rs`.

## Install/readiness harness pattern

- Source inventory is generated by `scripts/inventory-source.mjs`; project-shaped inventory is generated by `scripts/inventory-project.mjs`.
- `scripts/boot-harness.mjs` is the first install-readiness gate: inventory, source audit, npm pack, toolchain, binary distribution, and next actions.
- `scripts/install-readiness-harness.mjs` publishes the smaller `mcpace.installReadiness.v1` report.
- Published npm install readiness requires staged native binaries/platform packages; otherwise the npm package is a thin launcher/source-install artifact.

## v0.5.9 product-practice and proof-framing pattern

- Do not let feature/report accumulation imply runtime readiness. Source health, thin-launcher install, runtime beta, published binary install, and universal remote MCP brokering are separate claims.
- Keep `lint:npm` as a short auto-discovery harness (`scripts/check-node-syntax.mjs`) rather than a hand-maintained `node --check file && ...` list in `package.json`.
- Keep runtime proof as a named gate: `scripts/runtime-trace-harness.mjs` must stay blocked until a compiled/staged MCPace binary and real client/inspector trace prove initialize -> tools/list -> tools/call -> stdio upstream.
- Keep deterministic runtime proof fixtures under `tests/fixtures/`; `tests/fixtures/tiny-mcp-stdio-server.mjs` is the smallest stdio upstream for future trace capture.
- `START-HERE.md` is part of the clean source contract and should remain in `release-manifest.json`.
