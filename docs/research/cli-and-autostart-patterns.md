# CLI and autostart design research

Date: 2026-07-17

This note records the primary-source patterns used for MCPace's pre-1.0 CLI cleanup. It is design research, not copied implementation code.

## Result adopted by MCPace

The ordinary command surface is deliberately small:

```text
up | start | stop | restart | status | install | uninstall | advanced | help | version
```

Operator detail is grouped under:

```text
advanced doctor
advanced server ...
advanced client ...
advanced autostart ...
advanced runtime ...
advanced lease ...
advanced update ...
advanced dev ...
```

Generated MCP configs still need the hidden `stdio` entrypoint. Existing 0.8.x configs may still call hidden `stdio-shim` or `mcp-server`. Installed login entries need the exact hidden `agent run --autostart` contract, and legacy service entries may call hidden `serve --managed-service`. Those are compatibility/transport contracts and are intentionally excluded from human help.

All gratuitous aliases (`setup`, `quickstart`, `auto`, `server`, `client`, `dashboard`, `verify`, and similar top-level names) now fail instead of silently changing meaning.

## Primary open-source references

| Project | Primary source pattern | License | MCPace takeaway |
| --- | --- | --- | --- |
| [Tailscale](https://github.com/tailscale/tailscale/tree/main/cmd/tailscale/cli) | Stable task verbs such as `up`, `down`, and `status`; daemon is a separate executable. | BSD-3-Clause | Keep onboarding/status user-facing and keep daemon plumbing out of ordinary help. |
| [cloudflared](https://github.com/cloudflare/cloudflared/blob/master/cmd/cloudflared/linux_service.go) | Explicit `service install` / `service uninstall`; OS manager owns service lifecycle. | Apache-2.0 | Package installation, login registration, and runtime control are different operations. |
| [Syncthing](https://github.com/syncthing/syncthing/tree/main/cmd/syncthing/cli) and [autostart docs](https://github.com/syncthing/syncthing/blob/main/docs/users/autostart.rst) | Operational CLI is separate from systemd/desktop startup assets. | MPL-2.0 | Let the platform manager own boot/login behavior; expose product diagnostics around it. |
| [Ollama](https://github.com/ollama/ollama/blob/main/cmd/cmd.go) and [installer](https://github.com/ollama/ollama/blob/main/scripts/install.sh) | Small `serve`/`list`/`stop` vocabulary; Linux installer writes a systemd service that invokes a stable daemon command. Hidden login/logout compatibility commands demonstrate additive migration. | MIT | Keep stable hidden service entrypoints while simplifying the interactive vocabulary. |
| [Colima](https://github.com/abiosoft/colima/tree/main/cmd) | Public `start`, `stop`, `restart`, `status`; internal daemon command is hidden. | MIT | This is the closest lifecycle model for MCPace. Destructive removal is distinct from stop. |
| [Supabase CLI](https://github.com/supabase/cli/tree/develop/apps/cli-go/cmd) | `start`, `stop`, and `status` are scoped to one local stack. | MIT | Lifecycle commands should converge on a defined resource and status should be read-only. |
| [PM2](https://github.com/Unitech/pm2/blob/master/lib/binaries/CLI.js) | Runtime state (`start`, `stop`, `restart`, `delete`) is separate from boot integration (`startup`, `unstartup`, `save`, `resurrect`). | AGPL-3.0 | Reuse only the interface idea. Do not copy PM2 code into this Apache-2.0 project. |
| [Atuin](https://github.com/atuinsh/atuin/blob/main/crates/atuin/src/command/client/daemon.rs) | Explicit `daemon start|status|stop|restart`; bare deprecated form emits a warning. | MIT | Canonical nested actions and explicit migration are preferable to ambiguous aliases. |

Command names and behavioral patterns are interface ideas. MCPace does not copy source, unit files, command descriptions, or documentation text. Apache/MPL notice requirements and PM2's AGPL obligations are therefore not imported into MCPace.

## Lifecycle semantics

- `up`: convergent onboarding/repair; may patch supported clients and install/repair user login startup unless `--no-autostart`.
- `start`: start an existing configuration for this session; never patch clients or alter startup enablement.
- `stop`: stop supervisor/runtime now while preserving registration for the next login.
- `restart`: stop then start without changing configuration, clients, or registration.
- `status`: read-only aggregate runtime and login-registration report; exit nonzero when the configured runtime is stopped/unhealthy.
- `install`: add/update one upstream MCP server; never means package installation of MCPace itself.
- `uninstall`: remove local MCPace integration, current/legacy startup entries, verified MCPace-owned client entries, and ephemeral runtime state; preserve package, durable configuration, upstream definitions, and backups.

## Autostart evidence boundary

Authoritative platform sources:

- Windows [Run/RunOnce keys](https://learn.microsoft.com/windows/win32/setupapi/run-and-runonce-registry-keys), [Task Scheduler logon trigger](https://learn.microsoft.com/windows/win32/taskschd/logon-trigger), and [Windows Sandbox](https://learn.microsoft.com/windows/security/application-security/application-isolation/windows-sandbox/windows-sandbox-overview).
- Linux [systemd.user](https://www.freedesktop.org/software/systemd/man/latest/systemd.user.html), [systemd.service](https://www.freedesktop.org/software/systemd/man/latest/systemd.service.html), [systemd-run](https://www.freedesktop.org/software/systemd/man/latest/systemd-run.html), and [systemd-logind](https://www.freedesktop.org/software/systemd/man/latest/systemd-logind.html).
- WSL [boot settings](https://learn.microsoft.com/windows/wsl/wsl-config#boot-settings) and [systemd support](https://learn.microsoft.com/windows/wsl/systemd).
- Apple [Creating Launchd Jobs](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingLaunchdJobs.html) and the [`launchctl` manual](https://keith.github.io/xcode-man-pages/launchctl.1.html).

`mcpace advanced autostart prove` activates the exact registered manager target without rebooting and restores initial state. It proves registration/action/process identity, not fresh-login ordering. A real Windows Explorer login, PAM-created Linux user manager/lingering transition, WSL host boot, or first macOS GUI domain still requires a disposable VM/dedicated runner. See [`../platform-testing.md`](../platform-testing.md).
