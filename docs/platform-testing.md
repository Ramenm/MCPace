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

That workflow runs Node contracts, Rust tests, native binary smoke, and an isolated installed-runtime lifecycle (`init` → `up` → MCP initialize/tools/list → `serve stop`) on:

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
