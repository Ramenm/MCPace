# Install, autostart, and package-manager path

This project keeps one implementation core: the Rust `mcpace` binary.

## One-command local path

```bash
mcpace setup --json
```

This starts the local HTTP MCP endpoint, installs supported local client config
entries, and smokes `/healthz` plus `/mcp`.

Autostart is explicit opt-in:

```bash
mcpace setup --json --install-service
```

For CI or safe local proof without writing startup settings:

```bash
mcpace setup --json --install-service --no-enable
mcpace service install --json --dry-run
```

## Autostart implementation

`mcpace service ...` uses the `auto-launch` Rust crate instead of handwritten
per-OS startup files.

Current backend choices:

- Windows: current-user registry startup;
- macOS: user LaunchAgent;
- Linux/Ubuntu: user-level systemd.

The installed entry uses the current executable path and starts:

```bash
mcpace serve start --root <project> --host <host> --port <port>
```

Because the executable path is stored absolutely, autostart does not require the
`mcpace` command to already resolve through `PATH`.

## Distribution lanes

Now:

- source builds;
- thin npm launcher package;
- optional vendored binary inside the npm package for self-contained host-built
  artifacts.

Next:

- GitHub Release archives;
- npm platform packages.

Later, after signed artifact and install/uninstall proof:

- Homebrew;
- WinGet;
- Debian/Ubuntu `.deb` and optional APT repository.

Package managers should install `mcpace` into `PATH`; the runtime and autostart
logic should remain inside the Rust CLI so each package lane does not duplicate
OS-specific service code.
