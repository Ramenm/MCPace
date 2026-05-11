# Linux near-zero-touch checklist

Goal: a normal Linux user can install, run setup, get a local MCP endpoint, and check upstream servers without hand-editing JSON.

## Supported claim wording

Use this until musl artifacts are built and tested:

> MCPace supports Linux glibc x64/arm64. Alpine/musl and container/WSL autostart are supported only in degraded mode unless the matching proof lane passes.

## Host detection

- Detect `uname -s`, `uname -m`, Node `process.platform`, Node `process.arch`.
- Detect libc with `process.report.getReport().header.glibcVersionRuntime`, then `ldd --version` fallback.
- Detect package manager only for helpful instructions, not as a hard dependency.
- Detect `systemctl --user` and `$XDG_RUNTIME_DIR`.
- Detect WSL/container and downgrade autostart expectations.

## Bootstrap

- Create a user-owned root directory.
- Create `mcp_settings.json` if missing.
- Bind to `127.0.0.1`.
- Start with `setup --skip-client-install` for smoke.
- Check `/healthz` and MCP initialize/tools/list.

## Autostart

- Write `~/.config/systemd/user/mcpace.service` only when user systemd is available.
- Run `systemctl --user daemon-reload`.
- Run `systemctl --user enable mcpace.service`.
- Verify `systemctl --user is-enabled --quiet mcpace.service`.
- Use safe `ExecStart` escaping and avoid shell wrappers.
- Use `Restart=on-failure`, `RestartSec=5`, `NoNewPrivileges=true`, `UMask=077`.
- Document `loginctl enable-linger $USER` for boot-before-login/headless needs.

## Failure behavior

- Missing Rust/Docker/systemd should be `skip` or `warn` depending on profile, not silent pass.
- Wrong libc should be a warning/fail according to the release claim.
- Nonexistent project root for Serena should fail the Serena test.
