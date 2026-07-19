# Platform testing

MCPace has three proof levels. Do not mix them up.

1. **Local proof** proves the current machine only.
2. **GitHub platform proof** proves Linux, macOS, and Windows on hosted runners.
3. **Live MCP proof** proves one real client talking to `/mcp` and one real upstream server.

## Local proof on Linux or macOS

```bash
npm ci
npm run proof:local -- --full
```

If Cargo is missing, install Rust with rustup and rerun:

```bash
rustup toolchain install 1.95.0 --profile minimal --component rustfmt --component clippy
npm run proof:local -- --full
```

The report is written to:

```text
reports/local-proof-linux.md
reports/local-proof-darwin.md
```

depending on the host OS.

## Local proof on Windows PowerShell

Install prerequisites first:

```powershell
winget install OpenJS.NodeJS.LTS
winget install Rustlang.Rustup
```

Restart PowerShell so `node`, `npm`, `cargo`, and `rustc` are on `PATH`, then run from the project root:

```powershell
node --version
npm --version
rustup toolchain install 1.95.0 --profile minimal --component rustfmt --component clippy
npm ci
npm run proof:local -- --full
```

The report is written to:

```text
reports/local-proof-win32.md
reports/local-proof-win32.json
```

These reports contain host-specific paths and tool locations, so Git ignores them and they must not be committed or attached as portable release provenance. If the Windows proof fails, fix the first failing command shown in the report and rerun the same proof command.

## macOS without owning a Mac

Use the included GitHub Actions workflow:

```text
Actions → platform-proof → Run workflow → full=true
```

That workflow runs Node contracts, Rust tests, native binary smoke, and an isolated installed-runtime lifecycle (`up --no-autostart` → MCP initialize/tools/list → `stop`) on:

```text
ubuntu-latest
macos-15 (Apple Silicon)
macos-15-intel
windows-latest
```

The release workflow additionally builds and installs both macOS PKGs, validates their Mach-O architecture with `lipo`, records dependencies with `otool`, checks the package receipt, and runs the same isolated runtime lifecycle against `/usr/local/bin/mcpace`. Download the workflow artifacts after it finishes. The `platform-proof-report` artifact contains `reports/platform-proof.*`.

The checked-in `platform-proof` report is explicitly a **static plan contract**: it validates target declarations, workflow shape, command inventory, and smoke coverage. It does not claim that hosted runners executed. The workflow run conclusion plus its per-OS artifacts, bound to the release commit, are the execution evidence.

## What counts as done

A platform is considered locally proven only when all of these pass on that OS:

```bash
npm run check
npm run check:package
npm run release:dry-run
npm run pack:npm:dry-run
npm run build:release-artifacts
npm run check:rust
npm run build
npm run platform:binary-smoke -- --binary target/release/mcpace
```

`release:dry-run` validates only the tracked source-archive input and manifest policy; in dry-run mode it does not create the ZIP and it does not package the npm launcher. `pack:npm:dry-run` separately validates launcher packaging. Neither command rehearses the six native npm packages. Use the manual `publish-npm` workflow in dry-run mode for the full native package matrix and publish-contract checks.

On Windows, the final binary path is:

```text
target\release\mcpace.exe
```

The helper command `npm run proof:local -- --full` runs this sequence and writes the proof report.

## Autostart testing without rebooting a developer machine

`mcpace advanced autostart prove --json` is the product-level action proof. It does not rewrite registration: it records whether the runtime was running, stops it without disabling login startup, activates the exact installed OS target, verifies endpoint response plus PID/process identity, and restores the initial running/stopped state. Use `--dry-run` to inspect the target without stopping anything.

The destructive release harness, `scripts/autostart-lifecycle-proof.mjs`, also installs and removes the current user's login registration and kills a test runtime to prove recovery. It refuses to run unless both `--confirm-disposable-user` and `MCPACE_DISPOSABLE_AUTOSTART_PROOF=1` are present. Never set those on a contributor workstation; the workflows set them only on disposable hosted runner users.

The release matrix keeps claims separate:

| Platform | No-reboot CI proof | What still requires a disposable real session |
| --- | --- | --- |
| Windows | Verify HKCU Run value/plan; invoke the installed `mcpace-agent-launcher.exe --from-login`; assert launcher/agent identity and recovery. Windows Sandbox is acceptable for destructive smoke. | Sign into a disposable VM user and observe Explorer processing the Run entry. `schtasks /run` or manual launcher invocation is not a logon proof. |
| Linux | Install a temporary user unit; `systemctl --user daemon-reload`, enable/start/restart/stop; kill the child and assert `Restart=on-failure`. | PAM/login ordering and lingering behavior, when claimed, require a disposable VM user manager. A container/private manager is not equivalent. |
| WSL | In a disposable distro, enable systemd, run `wsl --shutdown`, relaunch the distro, and assert the user unit. | Windows host boot/login starting WSL is a separate Windows VM test; distro startup is not host boot. |
| macOS | On a hosted Mac with a GUI domain, use `launchctl bootstrap gui/$UID`, `print`, `kickstart -k`, kill/recovery, and `bootout` with a unique temporary label. | First GUI-session/login ordering, Keychain/Finder/WindowServer dependencies, quarantine, signing, notarization, and stapling require a disposable Mac VM or dedicated Mac. |

Every destructive test uses a temporary root inside a disposable OS user/runner, bounded timeout, observable marker/PID, and `finally`/`trap` cleanup. MCPace keeps its stable production login label, so OS-user isolation—not a renamed label—is the collision boundary. Manual manager activation is reported as lifecycle evidence, never mislabeled as reboot or fresh-login evidence. Actual login/reboot belongs in a separate nightly/release-validation workflow against disposable snapshots; it must never reboot a contributor's active workstation.
