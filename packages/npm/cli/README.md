# @mcpace/cli

Thin npm launcher for the MCPace native Rust binary.

Copy-paste install:

```bash
npm install -g @mcpace/cli@latest
mcpace up
```

Node.js 22+ is required. This is the only npm package users install: npm selects exactly one matching internal native optional package for Windows, glibc Linux, or macOS. The package resolves and launches `mcpace`; it does not duplicate the Rust runtime. Resolution order:

1. `MCPACE_BINARY_PATH` or `MCPACE_DEV_BINARY`
2. local source builds under `target/` or `dist/` when running inside the MCPace source workspace
3. optional platform packages such as `@mcpace/cli-linux-x64-gnu`

After the native binary is available:

```bash
mcpace up
```

`mcpace up` creates/repairs MCPace home, imports existing MCP servers from detected configs when safe, starts the endpoint, wires detected clients, and installs or repairs user-level autostart. It immediately transfers the runtime to the current user's supervisor: the hidden MCPace launcher on Windows, `systemd --user` on Linux, or a LaunchAgent on macOS. That supervisor also restores the endpoint after login and restarts failures. Use `mcpace up --no-autostart` for a session-only runtime. On WSL, systemd must be enabled and Windows must start the distribution before its Linux user service can run. It does not add a new upstream server by default.

## Remove MCPace

Run the product cleanup before removing the npm launcher:

```bash
mcpace uninstall --dry-run
mcpace uninstall
npm uninstall -g @mcpace/cli
```

`mcpace uninstall` stops the runtime, removes current and historical MCPace user-login entries, removes only ownership-verified MCPace client entries, and clears ephemeral runtime state. It preserves durable configuration, upstream definitions, and rollback backups. npm does not provide a reliable cross-platform uninstall hook for this OS lifecycle, so removing the package first can leave an inert startup entry behind.

Node floor: 22+.

## License

Apache-2.0. Copyright 2026 Ramenm.
