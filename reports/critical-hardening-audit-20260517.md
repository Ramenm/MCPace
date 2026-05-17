# Critical hardening audit, 2026-05-17

Status: **not publish-final**, but materially stronger than the previous archive.

## What was fixed in this pass

- `live-random-mcp-probe` is now schema v2 and covers npm plus PyPI/uv package-manager lanes.
- The probe client responds to `roots/list` and `ping` server requests instead of silently hanging during tool discovery.
- Package-manager probes support `--ids` to isolate flaky packages and `--force-canaries` for explicit slow/credentialed canary runs.
- Runtime launches use stripped environment variables and do not pass user secrets or the user home directory.
- npm installs use `--ignore-scripts`, `--no-audit`, `--no-fund`, `--omit=dev`.
- PyPI installs happen inside a disposable `uv` venv.
- Consolidated evidence now covers 12 package entries: 8 npm entries and 4 PyPI entries, with 10 successful live probes, 91 discovered tools, 1 expected credential/startup failure, and 1 skipped policy canary.
- The fixture replay contract was updated to the new schema and now checks npm + PyPI coverage.

## Real problems found

1. **Full random npm matrix is not deterministic enough for default CI.** Some large or credentialed packages can hang under restricted mirrors or leave handles. They are now canaries, not default blockers.
2. **The previous live probe was too optimistic.** It did not handle server-initiated `roots/list`, which can make otherwise valid MCP servers hang during discovery.
3. **Tool annotations cannot be treated as proof.** The classifier must keep using names, descriptions, transport, package type, stderr, and user review in addition to annotations.
4. **No destructive/random tool calls are safe by default.** The current lane is tool discovery only. A future lane must use a stronger sandbox and explicit fixtures before calling tools.
5. **Rust source still needs rebuild proof.** Node evidence is green, but without `cargo`/`rustc` this environment cannot prove that Rust source changes are compiled into the vendored binary.
6. **Docker lane remains unverified here.** Docker is not available in this host, so Docker-based MCP packages are still a separate CI lane.
7. **UI source may be ahead of vendored binary.** Dashboard source changes require a Rust rebuild before the distributed binary can honestly claim them.

## Release posture

- OK to publish as a source/beta hardening archive.
- Not OK to claim full production-safe random MCP execution.
- Not OK to claim Docker coverage from this host.
- Not OK to claim Rust rebuild/source parity until Rust CI runs.

## Next gates before a public release claim

1. Rust CI: `cargo fmt`, `cargo clippy`, `cargo test`, release build.
2. Default binary/source parity check after rebuild.
3. Docker/chroot/firejail sandbox lane for destructive fixtures.
4. Concurrency torture with real filesystem, git, browser, SQLite, and memory servers.
5. UI E2E against rebuilt dashboard binary.
6. Process-tree cleanup outside Node for hostile packages.
