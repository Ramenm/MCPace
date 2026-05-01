# MCPace docs

This packaged copy tracks repo version `0.4.1`.

MCPace is a Rust-first local MCP hub. It ships with no upstream MCP servers enabled by default and no hardcoded recommended upstream catalog. Configure user-supplied stdio MCP servers directly in `mcp_settings.json`; add `mcpace.config.json` server policy only when you need extra routing, platform, or tool-risk metadata.

Start with:

- `product-truth.json` for the machine-readable product promise.
- `mcp-spec-alignment.md` for the checked MCP baseline.
- `client-surface-matrix.md` and `client-metadata-routing.md` for client routing.
- `server-segmentation-and-auto-discovery.md` for server discovery and serialization.
- `test-strategy.md` and `verification-matrix.md` for checks.
- `adr/0004-source-only-mcp-env-isolation.md` for the source-only MCP env isolation decision.
- `adr/0005-ci-cache-and-upstream-diagnostic-redaction.md` for Cargo CI caching and stderr diagnostic redaction.

Run from the repository root:

```bash
npm test
npm run verify:rust-quality
cargo fmt --all -- --check
cargo check --all-targets --locked
cargo test --all-targets --locked
```

Minimal source-only upstream example:

```json
{
  "mcpServers": {
    "my-server": {
      "command": "node",
      "args": ["path/to/server.js"],
      "env": { "EXPLICIT_VAR": "value" },
      "env_vars": ["TOKEN_FROM_PARENT_ENV", { "name": "LOCAL_TOKEN", "source": "local" }],
      "cwd": "/absolute/or/project-specific/path"
    }
  }
}
```

Stdio upstream children get a cleared environment plus a small process-launch baseline, MCPace runtime variables, and explicit `env` / local `env_vars` values only. This preserves generic MCP server support without forwarding every parent process secret by default.
Cache/session fingerprints hash explicit env values so plaintext tokens are not embedded in cache keys.
Upstream stderr included in errors is bounded and sanitized before display so diagnostics remain useful without intentionally echoing obvious credentials.
