# MCPace full project verification checklist

This is the baseline checklist before saying the project is healthy on a target machine.

## 0. Clean source and release hygiene

- [ ] Source tree does not contain `.claude/`, `.codex/`, `.omc/`, `%SystemDrive%/`, screenshots, local caches, or `.env` with real secrets.
- [ ] `npm run verify:secrets -- --json` returns zero findings.
- [ ] `npm run audit:source` has no critical findings.
- [ ] Release archive is produced by the maintained archive script, not by manually zipping a working directory.

## 1. Host prerequisites

- [ ] `node --version`, `npm --version`, and `npx --version` work.
- [ ] Rust toolchain is installed when building from source: `cargo --version`, `rustc --version`.
- [ ] Linux: libc is detected and the artifact target matches it (`glibc` vs `musl`).
- [ ] Windows: `npx.CMD` path with spaces is tested.
- [ ] macOS: downloaded artifacts are signed/notarized before public release.

## 2. Build and source tests

- [ ] `npm run lint:npm`
- [ ] `npm run test:repo:smoke`
- [ ] `npm run test:npm`
- [ ] `npm run test:rust:ci` when Rust is available.
- [ ] `npm run verify:rust-quality` when Rust tooling is available.

## 3. Install and packaging

- [ ] `npm run verify:npm-pack`
- [ ] `npm run verify:platform-packages`
- [ ] Linux/macOS packaged binary has executable bit.
- [ ] Native package target matches `process.platform`, `process.arch`, and Linux `libc`.
- [ ] Install from packed tarball works in an empty temp directory.

## 4. MCP HTTP flow

- [ ] Start MCPace on `127.0.0.1`, not `0.0.0.0`.
- [ ] `GET /healthz` works.
- [ ] `POST /mcp` `initialize` returns protocol version.
- [ ] If `MCP-Session-Id` is returned, every following MCP HTTP request sends it.
- [ ] `notifications/initialized` is accepted.
- [ ] `tools/list` returns expected hub tools such as `hub_status`.
- [ ] GET or unsupported methods on `/mcp` are rejected.
- [ ] Missing/duplicate `Host`, conflicting `Content-Length`, invalid `Content-Type`, and unsupported `Transfer-Encoding` are rejected.

## 5. Upstream MCP servers

- [ ] `mcpace server list --json` shows only intended enabled servers.
- [ ] Every `npx`/`npx.CMD` server has `env_vars` for npm registry/cache/proxy/cert variables.
- [ ] API-backed servers use `env_vars` names, not literal secret values in JSON.
- [ ] Serena uses a real project root, real `cwd`, project config, and a long timeout.
- [ ] Run `mcpace server test <name> --refresh --timeout-ms 30000 --json` for each enabled server.
- [ ] Run Serena with a longer timeout, for example `120000` ms.

## 6. Linux automatic flow

- [ ] `npm run verify:linux:auto:host` on the host.
- [ ] `scripts/linux-auto-setup.sh --root /tmp/mcpace-user-test --bin ./target/release/mcpace --skip-client-install`.
- [ ] If `systemd --user` exists: unit is written, `daemon-reload` ran, unit is enabled, and restart works.
- [ ] If `systemd --user` does not exist: setup produces an explicit degraded warning, not a fake pass.
- [ ] Containers/WSL/minimal distros are documented as degraded if user systemd is unavailable.

## 7. Security baseline

- [ ] Default bind address is localhost.
- [ ] Non-local bind requires explicit user opt-in and real authentication.
- [ ] Command spawning avoids shell interpolation where possible.
- [ ] Any `.cmd`/`.bat` Windows launch path is tested with spaces and metacharacter-like arguments.
- [ ] Logs redact secrets and keep stdout clean for stdio MCP upstreams.
- [ ] Dashboard uses conservative browser headers where applicable.

## 8. Release proof

- [ ] Ubuntu/Debian glibc x64 smoke.
- [ ] Linux ARM64 smoke.
- [ ] Alpine/musl smoke only if musl artifacts are published.
- [ ] Windows x64 and ARM64 smoke.
- [ ] macOS Intel and Apple Silicon smoke.
- [ ] Published npm packages are dry-run packed and install-tested.
- [ ] Checksums are generated and verified.
