# MCPace security hardening checklist

## HTTP/MCP ingress

- Bind default local server to `127.0.0.1`.
- Reject non-local binds unless the user explicitly enables them.
- Add authentication before treating non-local HTTP transport as safe.
- Validate `Origin` on MCP HTTP requests.
- Validate exactly one `Host` header.
- Reject conflicting `Content-Length` headers.
- Reject unsupported `Transfer-Encoding` until chunked parsing is implemented.
- Require `Content-Type: application/json` for MCP POST requests.
- Treat `MCP-Session-Id` as a session routing token, not as authentication.

## Process spawning

- Prefer direct argv arrays over shell command strings.
- Never concatenate untrusted values into `pwsh -Command`, `cmd.exe /C`, or `sh -c`.
- On Windows, regression-test `.cmd`/`.bat` launchers with paths containing spaces.
- Use env allowlists (`env_vars`) and redact secret values in diagnostics.
- Keep MCP stdio stdout protocol-clean; send logs to stderr.

## Config and secrets

- Store API-key variable names in config via `env_vars`; do not store values.
- Back up before modifying user config.
- Redact known secret patterns in reports.
- Do not include local state directories in source/release archives.

## Release artifacts

- Linux/macOS binaries must preserve executable bit.
- Linux glibc floor must be verified before claiming older distro support.
- macOS artifacts intended for public download should be signed and notarized.
- Checksums should be generated for release artifacts and verified in CI.
