# Library simplification audit

This audit tracks handwritten areas that should become library-backed only when
the replacement clearly reduces maintenance risk.

## Done now

| Area | Previous risk | Library-backed path |
|---|---|---|
| User autostart | OS-specific startup registration is easy to get subtly wrong on Windows, macOS, and Linux. | `auto-launch` owns current-user Windows registry startup, macOS LaunchAgent, and Linux user-systemd/XDG behavior behind `mcpace service ...`. |
| Executable discovery | Manual `PATH`/`PATHEXT` scanning had to duplicate platform command lookup rules. | `which` now resolves tool paths for `doctor`/setup diagnostics. |
| JSON parsing/printing | A local JSON parser/formatter must correctly handle escapes, unicode surrogate pairs, number grammar, and error reporting forever. | `serde_json` now owns parsing and serialization behind MCPace's existing `JsonValue` compatibility wrapper. |

## Good next candidates

| Area | Candidate library | Why not all at once |
|---|---|---|
| CLI argument parsing | `clap` | There are many small `parse_args` functions. Replacing them is worthwhile, but should be one command family at a time so help text and exit codes do not drift. |
| TOML config patching | `toml_edit` | Better than string-built TOML blocks, but must preserve existing user config and MCPace managed-block idempotency. |
| YAML config patching | `serde_yml` or `yaml-rust2` | Better than manual section scanning, but needs comment/idempotency tests first. |
| HTTP client probes | `ureq` | Would simplify setup smoke probes. Defer because current probes are localhost-only and `ureq` brings a larger HTTP/TLS dependency graph. |
| HTTP server framing | `tiny_http` first, `axum` later if async runtime lands | Dashboard/serve hand-roll HTTP parsing. This is runtime-critical and needs endpoint-level regression tests before replacement. |
| User directories | `home` or `directories` | Can centralize `~`/home resolution, but existing tests rely on explicit `HOME`/`USERPROFILE` isolation, so migrate with test coverage. |

## Current rule

Prefer a small, popular, proven crate when it deletes platform rules or parsers
that MCPace should not own. Do not add a large runtime framework just to make a
small local-only path look cleaner.
