# MCPace Linux verification checklist

This checklist is the baseline for saying that MCPace is safe and usable on Linux. It is intentionally practical: a maintainer should be able to run one command, read one report, and know what still blocks release.

## One-command check

From the repository root:

```bash
npm run verify:linux:auto
```

For a host-only run without Docker:

```bash
npm run verify:linux:auto:host
```

For the full Linux gate:

```bash
npm run verify:linux:auto:full
```

For user-level bootstrap on a clean Linux machine:

```bash
npm run setup:linux:auto -- --skip-client-install
```

The checker writes:

```text
reports/linux-auto-check-latest.json
reports/linux-auto-check-latest.md
```

## Pass/fail rule

A Linux release is not ready unless all required checks pass. Skips are allowed only when the report explains why, for example Docker is unavailable on the current machine. Warnings are allowed for local development, but must be reviewed before release.

## 1. Fresh machine and repository hygiene

- Clone/extract the repository into a new directory.
- Confirm no local machine-state directories are in release inputs: `.claude`, `.codex`, `.omc`, `%SystemDrive%`, `node_modules`, `target`, `logs`, `backups`.
- Confirm `release-manifest.json` does not include local/private paths.
- Remove root-level screenshots from source snapshots.
- Confirm `SECURITY.md`, `LICENSE`, `Cargo.lock`, `package.json`, and `release-targets.json` are present.
- Run the secret scan before publishing any archive.

Command:

```bash
npm run verify:secrets -- --json
```

## 2. Linux platform support definition

- Supported today: glibc Linux targets in `release-targets.json`.
- Planned but not release-ready until proven: musl/Alpine targets.
- The release notes must not claim “all Linux distributions” until musl packages and Alpine install proof pass.
- For glibc binaries, record the maximum `GLIBC_*` symbol version and test on the oldest glibc distribution you claim to support.

## 3. Toolchain preflight

Required on a build machine:

- Node.js matching `package.json` engines.
- npm matching `package.json` engines.
- Rust toolchain for the configured target.
- Docker for distro/install proof.
- Git for release/provenance metadata.

Command:

```bash
npm run verify:linux:auto:host
```

## 4. Source and npm surface

Run:

```bash
npm run lint:npm
npm run test:npm
npm run test:repo:smoke
npm run verify:release-targets
npm run verify:platform-packages
npm run verify:npm-pack
```

Expected:

- Node syntax check passes.
- npm launcher tests pass.
- release target manifest is valid.
- platform package metadata is synchronized.
- npm pack dry-run includes expected files.
- Unix binaries have executable bits.

## 5. Rust build/runtime proof

Run when Rust is available:

```bash
npm run test:rust:ci
cargo build --release
./target/release/mcpace version
./target/release/mcpace doctor --json
```

Expected:

- Version equals `package.json`/Cargo version.
- Doctor reports source/runtime readiness.
- No command writes outside expected config/state/cache/log locations.

## 6. Clean Linux npm install proof

Run:

```bash
npm run test:linux-npm-install:docker
```

Expected:

- Builds the Linux binary.
- Stages the matching platform package.
- Packs `@mcpace/cli` and the platform package.
- Installs both into a clean consumer project.
- Runs `node_modules/.bin/mcpace version` successfully.

## 7. Distro compatibility proof

Minimum proof before claiming Linux support:

- Current Ubuntu LTS image.
- Previous Ubuntu/Debian glibc image, or another image with the oldest supported glibc.
- ARM64 Linux runner or equivalent native/proven emulation for `linux-arm64-gnu`.

Do not claim Alpine support unless a musl target is enabled and install/runtime proof passes on Alpine.

## 8. MCP runtime smoke

Run:

```bash
mcpace serve stop --json --root "$PWD" || true
mcpace setup --json --root "$PWD" --host 127.0.0.1 --skip-client-install
```

Then perform:

- `initialize` over `POST /mcp`.
- Capture `MCP-Session-Id` when the server returns one.
- Send `notifications/initialized` with the session header.
- Send `tools/list` with the session header.
- Confirm `GET /mcp` behaviour matches the supported transport mode.

## 9. Upstream MCP server proof

For every enabled upstream server:

```bash
mcpace server test <name> --refresh --timeout-ms 30000 --json
```

For project-aware servers such as Serena, use a real project root and a longer timeout:

```bash
mcpace server test serena --refresh --timeout-ms 120000 --json
```

For `npx` upstream servers, config must use `env_vars` names rather than embedding secret values. At minimum pass through npm registry/proxy/cert variables when present.

## 10. Linux security baseline

- Bind local MCP HTTP endpoints to `127.0.0.1` by default.
- Treat `0.0.0.0` as unsafe unless a real authentication layer is enabled.
- Validate HTTP `Host`, duplicate `Host`, duplicate/conflicting `Content-Length`, unsupported `Transfer-Encoding`, and MCP `Content-Type`.
- Do not treat session IDs as authentication.
- Do not log API keys or full upstream environment values.
- Keep upstream child environment allowlisted and explicit.
- Store user-specific config/state/cache according to XDG-compatible locations where possible; keep legacy `~/.mcpace` only as compatibility state.

## 11. Supply-chain and release proof

Before public release:

```bash
npm run verify:supply-chain
npm run verify:publish-readiness
npm run generate:checksums
npm run verify:release-checksums
```

Release artifacts should have:

- Checksums.
- Build provenance/attestations.
- npm Trusted Publishing or another short-lived/OIDC-based publishing path.
- No long-lived npm token in CI.
- A documented verification command for users.

## 12. Human first-run flow

On a clean Linux machine, a human should be able to do this without editing JSON by hand:

```bash
npm install -g @mcpace/cli
mcpace version
mcpace doctor --json
mcpace setup --json --host 127.0.0.1 --skip-client-install
mcpace server install npm:@modelcontextprotocol/server-filesystem --as filesystem --path . --dry-run --json
mcpace server install filesystem --env-var NPM_CONFIG_REGISTRY --env-var NPM_CONFIG_USERCONFIG
mcpace server test filesystem --refresh --timeout-ms 30000 --json
```

If any step fails, the error should show the actual executable, cwd, sanitized args, exit code, stderr tail, timeout, and which env var names were passed through.
