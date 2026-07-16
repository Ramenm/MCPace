# Changelog

All notable user-facing changes are recorded here. Keep entries focused on behavior, installation/release impact, compatibility, security, and migration rather than every internal refactor.

## Unreleased

### Changed

- `mcpace up` now installs or repairs user-level autostart by default; `--no-autostart` keeps an explicit session-only path.
- Linux persistence now uses `mcpace-agent.service` under `systemd --user` with restart-on-failure instead of desktop-only XDG Autostart.
- On Windows and Linux, `mcpace up` now activates the user supervisor immediately and verifies the managed endpoint before returning, instead of leaving a detached owner until the next login.

### Fixed

- Windows setup now repairs stale Run commands and missing autostart plans during the normal `mcpace up` flow.
- The hidden Windows launcher now supervises non-zero agent exits with bounded backoff, and root discovery can recover from the short launcher-plan Run entry.
- Same-configuration `serve restart` now preserves systemd/Windows supervisor ownership and uses an acknowledged stop handshake to prevent duplicate runtime starts.
- Autostart migration removes known legacy Windows Startup-folder launchers and Linux XDG entries to avoid duplicate owners.

## 0.8.2 - 2026-07-16

### Changed

- Synchronized the Rust crate, npm launcher, native optional dependency pins, lockfiles, and project configuration at version `0.8.2`.
- Repacked the source tree with one named root, without Git metadata, dependencies, caches, runtime state, agent transcripts, or build outputs.

### Verification

- Re-ran the complete 447-test Node inventory file by file so early failures could not hide later failures.
- Kept stale Rust/live-proof and source-without-Git release gates fail-closed instead of marking the package production-ready.

### Security

- No production security boundary is claimed fixed by this packaging patch. Mandatory HTTP identity, approval receipts, principal-bound leases, centralized SSRF controls, cancellation propagation, and sandboxing remain release blockers.

## 0.8.0 - 2026-07-14

### Added

- Added one canonical dashboard shell with **Home**, **Integrations**, **Applications**, **Activity**, and **Settings**, plus bounded retained-operation views and reusable upstream-session visibility.
- Added explicit execution policy, lease-queue, HTTP session, and release-readiness contracts with focused regression tests.

### Changed

- Split the framework-free dashboard into bounded runtime/model/render/details/actions/boot/product chunks while preserving explicit load order and Rust embedding.
- Reworked pooled stdio execution around per-worker checkout, failure invalidation, capacity eviction, configured idle TTLs, and real `maxWorkers` concurrency.
- Made stable npm publication tag-only and immutable-SHA aware; kept GitHub installer publication draft-only; and made both lanes provenance/attestation gated and fail-closed around incomplete native package sets.

### Security

- Hardened HTTP framing and absolute deadlines, rejected requests before reading unauthorized bodies, bounded JSON-RPC replay state by count and bytes, and made stdio invalid UTF-8 fail closed.
- Tightened dashboard HTML/URL sanitization, modal keyboard handling, CSV formula neutralization, source-archive hygiene, and generated-agent transcript exclusion.

### Fixed

- Preserved launcher arguments such as `npx -y`, corrected MCP HTTP method/envelope errors, coalesced overview refreshes, and bounded retained operation lines/events.
- Fixed clean-root setup, endpoint flag persistence, cross-platform autostart endpoint reuse, positional server-spec parsing, and release/workflow policy parsing.

### Migration

- `mcpace stdio` is canonical. Existing `stdio-shim` client entries remain compatible for now; follow `docs/supported-clients.md` to preview and apply the replacement before a future major release.

## 0.7.9 - 2026-07-06

### Fixed

- Bumped the immutable npm version so the Windows hidden-autostart repair from `0.7.8` reached npm `latest`; aligned Cargo, launcher, optional native dependency pins, lockfiles, and project config.

## 0.7.8 - 2026-07-03

### Added

- Added npm publish automation for unique `dev` prereleases and stable releases.

### Fixed

- Fixed Windows npm-installed autostart so MCPace registers the current-user `MCPace` entry and removes the legacy Startup-folder `MCPace.cmd` launcher.

## 0.7.7 - 2026-06-25

### Fixed

- Ensured native optional packages no longer declare a competing `mcpace` bin entry; the launcher package is the sole owner of the user-facing command.

## 0.7.6 - 2026-06-25

### Fixed

- Restored executable mode for the npm launcher shim so fresh `@mcpace/cli` installs create the `mcpace` command without `npm rebuild`.

## 0.7.5 - 2026-06-25

### Fixed

- Applied Rust 1.95.0 formatting and Clippy cleanups for hardened dashboard/service paths.

### Verification

- Added verification evidence for formatting, Clippy, Rust tests/build, Node checks, external tooling, load proof, and source-bundle packaging.

## 0.7.4 - 2026-06-25

### Changed

- Bumped the verified source bundle after the deep toolchain and logic recheck.
- Made the load harness admission-aware and preserved per-scenario runtime snapshots.
- Hardened external tooling preflight, normalized the documentation set, removed stale release-manifest paths, aligned repository labels/workflow metadata, and tightened npm package-file metadata.

### Fixed

- Restored dashboard and npm launcher tests, updated stale dashboard/UI contracts, aligned release reports, expanded repository hygiene coverage, and normalized evaluation-ledger newlines.

## 0.7.0

### Changed

- Finalized the dashboard foundation order around Backend, Client, Source, Tools, and Routing instead of cockpit-style status walls.
- Kept import, discovery, client wiring, automation, protocol diagnostics, and access review in folded secondary layers.
- Made `dashboardFoundation` the backend-owned source of truth for the first-screen setup state.
- Aligned disabled imported sources with the safe flow: preview, save disabled, review, enable, then test before use.

### Fixed

- Prevented empty client identifiers from counting as wired clients.
- Prevented saved-but-disabled sources from counting as routable sources.
- Kept routing readiness conservative until an enabled source, tool evidence, runtime readiness, and policy health are present.

## 0.6.9

### Added

- Node-side source-bundle checks for npm package integrity, release artifact creation, GitHub metadata labels, line endings, local script references, Rust module reachability, helper duplication, and bundled hub-example/schema drift.
- Node-only release ZIP writer/reader so source artifact creation no longer depends on external `zip` or `unzip` binaries.
- npm launcher shim at `packages/npm/cli/bin/mcpace.js` with Node-version guard and native-binary resolution.
- Local load-test binary discovery now supports `MCPACE_BINARY_PATH` / `MCPACE_DEV_BINARY` in addition to `--binary`.

### Changed

- Consolidated release/CI workflows to commands that exist in this source bundle.
- The npm launcher only considers `target/` or `dist/` development binaries when running from the MCPace source workspace, not from arbitrary consuming projects.
- Kept the repository source-only: no prebuilt native binaries, `node_modules`, Rust `target`, runtime state, logs, caches, or stale public-repo scaffolding are included.
- Centralized Rust helpers for runtime paths, CLI text formatting, environment parsing, platform aliases, notification-method detection, and Windows command-line quoting.
- Aligned GitHub labels across Dependabot, issue templates, release categories, and workflow hygiene tests.
- Normalized documentation around the install/user path: short landing README, focused docs under `docs/`, and explicit final-validation steps for Rust-capable hosts.
- Removed retired legacy bridge surfaces from the source bundle: `manager.settings.json`, hub bridge flags, and opt-in projected-tool top-level controls.

### Fixed

- Fixed broken source-package references to missing files and missing npm/Node scripts.
- Fixed Windows-sensitive npm/npx launching by resolving `.cmd` wrappers where needed and by keeping service-launcher argument quoting centralized.
- Fixed hub example/schema drift: the bundled `examples/mcpace-hub.*.json` files are allowed to start with zero upstream servers and empty manual profiles.
- Fixed profile selection drift by using `mcpace.config.json` as the single source of runtime-profile truth, with `MCPACE_RUNTIME_PROFILE` as the explicit override.
- Fixed stale generated-code comments and CRLF text files that contradicted `.editorconfig`/`.gitattributes`.
- Fixed `npm run load:local` guidance and error reporting when the Rust binary has not been built yet.
- Home import recognizes the normalized MCPace self-entry name `mcp-pace` and skips it to avoid loops.
- MCP config import accepts URL aliases (`serverUrl`, `httpUrl`, `endpoint`) and normalizes remote type aliases to `streamable-http`.

### Still requires Rust-host validation

- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`
- `cargo build --release`
- `npm run load:local -- --binary ./target/release/mcpace --duration-ms 5000 --concurrency 64`

Reason: this source bundle does not ship a Rust toolchain or prebuilt native binary.
