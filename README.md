# MCPace

MCPace is a Rust-first local MCP hub for many clients. It keeps a single local MCP hub for many clients, exposes one local MCPace endpoint, and brokers upstream MCP servers generically with no bundled upstream MCP servers enabled or recommended by default. The repository tracks the MCP 2025-11-25 baseline. Current promise: One local MCPace endpoint and generic MCP brokering/diagnostics with no bundled upstream MCP servers enabled or recommended by default.

`serve` is the product. `hub` is internal/operator-facing lifecycle machinery. The default runtime profile is `manual`, so `mcp_settings.json.mcpServers`, `mcpace.config.json.servers`, and the packaged candidate catalog start empty.

## Configure any upstream MCP server

The product model is **Bring Your Own MCP servers (BYO MCP)**: MCPace ships the
hub/adapter, not a bundled upstream-server catalog. Other users do not need a
new MCPace build when they choose different servers. They install whatever
upstream MCP packages or binaries they trust, then add those entries to their
own `mcp_settings.json`.

Add user-supplied servers explicitly in `mcp_settings.json`. MCPace accepts ordinary stdio-style MCP entries without also requiring a hardcoded server declaration in `mcpace.config.json`:

```json
{
  "mcpServers": {
    "my-server": {
      "command": "node",
      "args": ["path/to/server.js"],
      "env": {
        "EXPLICIT_VAR": "value"
      },
      "env_vars": ["TOKEN_FROM_PARENT_ENV"],
      "cwd": "/absolute/or/project-specific/path"
    }
  }
}
```

`enabled` defaults to `true` for user-supplied entries. `type` can be omitted for stdio commands; MCPace infers `stdio` from `command` and HTTP-like transport from `url`. Add a matching entry in `mcpace.config.json` only when you want optional policy metadata such as routing class, concurrency, platform gating, required commands, or tool risk gates.

Portability contract for other users:

- packaged `mcp_settings.json.mcpServers`, `mcpace.config.json.servers`, and
  `server-candidates.json` stay empty;
- new server names are discovered from the user's `mcp_settings.json` at
  runtime, without recompiling MCPace;
- `mcpace server list --json`, `upstream_probe`, `upstream_catalog`,
  `upstream_tools`, `upstream_call`, and `upstream_batch` operate on those
  configured names;
- MCPace does not install upstream packages for the user; it reports missing
  commands so the user can install the server in their preferred way.

Stdio upstream processes do not inherit the full MCPace parent environment. MCPace clears the child environment, adds a small process-launch baseline, then applies explicit `env` values and allowlisted local `env_vars` values. `env_vars` accepts either string names or local object entries such as `{ "name": "TOKEN", "source": "local" }`. Add secrets explicitly; do not rely on accidental parent-env inheritance.
Runtime cache fingerprints include env variable names and hashed values only, not plaintext secret values.
When upstream startup, timeout, or JSON-RPC errors include stderr, MCPace keeps bounded diagnostic context but redacts likely tokens, passwords, credentials, API keys, private keys, Authorization values, and bearer tokens before surfacing the error.

HTTP/Streamable HTTP entries are inventoried and reported, but the current stdio bridge only forwards callable stdio upstreams. HTTP upstream fan-out remains blocked honestly in diagnostics until implemented.

## Useful commands

```bash
npm test
npm run verify:rust-quality
npm run benchmark:runtime
cargo fmt --all -- --check
cargo check --all-targets --locked
cargo test --all-targets --locked
cargo build --release --locked
```

In constrained sandboxes, local Node checks can run with the project npm surface. Full Cargo check/test/build need the pinned Rust toolchain and crates.io dependencies available through the configured network/cache. CI Rust jobs cache Cargo registry/git/target with keys derived from OS, Rust version, target or suite, `Cargo.lock`, and `rust-toolchain.toml`.

## Read paths and diagnostics

Runtime HTTP controls remain explicit: `--max-connections`, `--io-timeout-ms`, `--max-body-bytes`, and `--overview-cache-ms`.


- `mcpace client plan --json` shows the client plan.
- `mcpace server list --json` shows both policy-declared servers and source-only `mcp_settings.json` servers.
- `mcpace server capabilities --json --name <server>` shows transport, command, source, and policy details.
- `mcpace verify readiness --json` reports missing stdio commands for source-only servers.
- `mcpace lab report --json` writes a lab report.
- `mcpace release build --json` creates local release artifacts and does not publish.

## Not implemented yet

Network publication, HTTP upstream fan-out from the stdio bridge, cloud relay support as a product lane, and enterprise/team policy management are not implemented yet. Keep those claims out of product docs until they have proof artifacts.
