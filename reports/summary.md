# MCPace v0.6.9 packaging summary

## What changed in this pass

- Normalized the document set around the public surface: concise root README, compact runbook, focused architecture/configuration/security/client/troubleshooting docs, and this summary.
- Removed bundle-only noise that did not help installation or verification: stale `CITATION.cff`, `CODE_OF_CONDUCT.md`, `CONTRIBUTING.md`, `SOURCE_ARCHIVE_NOTE.txt`, and empty platform-package scaffolding directories.
- Kept useful docs and configs: `README.md`, `docs/README.md`, focused docs, `SECURITY.md`, `CHANGELOG.md`, examples, schemas, package metadata, and source configs.
- Added Node test coverage for docs/package hygiene, version alignment, forbidden artifact checks, and MCP import-normalization guards.
- Updated `npm run check` to run Node syntax checks plus Node tests.
- Tightened MCP config import normalization: `servers` and `mcpServers` both accept `url`, `serverUrl`, `httpUrl`, and `endpoint`; URL aliases become `url`; remote type aliases normalize to `streamable-http`; `disabled: true` maps to `enabled: false`.
- Fixed MCPace self-entry detection for the normalized name `mcp-pace` in home import.
- Shortened `mcpace help` so the visible surface remains centered on `up`, `install`, `serve`, `server`, `client`, `connect`, and `doctor`.

## Verification performed here

- `npm run lint:npm`: passed.
- `npm run test:npm`: passed.
- `npm run check`: passed.
- `npm run pack:npm:dry-run`: passed.
- ZIP integrity check: passed.

Rust build/tests were not run in this environment because `cargo`/`rustc` are unavailable. The new Rust unit tests are included for hosts with the Rust toolchain.

## Package hygiene

The ZIP is a source bundle with one root directory. It excludes `.git`, `node_modules`, caches, temporary files, OS artifacts, runtime logs/data/backups, vendored platform binaries, and heavyweight build outputs.
