# Changelog

All notable user-facing changes should be recorded here. Keep this file human-readable: focus on behavior, install/release impact, compatibility, security, and migration notes rather than every internal refactor.

## Unreleased

### Changed

- Normalized the documentation set into a compact landing README, focused docs under `docs/`, and a condensed source-bundle summary.
- Removed stale report paths from the release manifest so source artifact verification matches the files that actually ship.
- Aligned GitHub issue-template labels with the declared repository label taxonomy and removed stale documentation placeholders.
- Rechecked every shipped documentation/governance surface and aligned GitHub artifact download and security workflow metadata with the current workflow contract.
- Tightened npm package-file metadata so the thin CLI package only declares source paths that exist in the release bundle.

### Fixed

- Restored the npm CLI executable shim expected by `@mcpace/cli` package metadata and release artifact tests.
- Extended repository hygiene coverage so every issue template is checked for undeclared labels.
- Added hygiene coverage for security workflow trigger reachability and artifact upload/download action-major alignment.
- Normalized final newlines in the large evaluation ledgers so text-file audits are clean.

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
