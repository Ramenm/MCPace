# MCPace

MCPace is a Rust-first local MCP hub for many clients. It keeps a single local MCP hub for many clients, exposes one local MCPace endpoint, and brokers upstream MCP servers generically with no bundled upstream MCP servers enabled or recommended by default. The repository tracks the MCP 2025-11-25 baseline. Current promise: One local MCPace endpoint and generic MCP brokering/diagnostics with no bundled upstream MCP servers enabled or recommended by default.

`serve` is the product. `hub` is internal/operator-facing lifecycle machinery. The default runtime profile is `manual`, so `mcp_settings.json.mcpServers`, `mcpace.config.json.servers`, and the packaged candidate catalog start empty.

## First working path

For the top-level proof order, read `START-HERE.md` and `docs/product-practice.md`.


From a user/client point of view, the shortest safe path is:

```bash
mcpace connect
mcpace server presets
mcpace server starter --path . --dry-run
mcpace server starter --path .
mcpace server test filesystem --refresh --json
mcpace client install cursor-local --dry-run --diff
```

Use `mcpace connect [client] --server <name>` whenever you are unsure what to do next. It is read-only and reports the resolved MCPace endpoint, selected client target, upstream MCP source inventory, readiness blockers, and exact next commands. Use `server starter` for the smallest useful local setup, `server presets` to inspect editable preset data, `server import` when you already have a client or project MCP config, `server add` for a fully custom BYO stdio MCP server, `server test` before wiring a client, and `client export` / `client install` only after the upstream smoke is clear.

## Configure any upstream MCP server

The product model is **Bring Your Own MCP servers (BYO MCP)**: MCPace ships the
hub/adapter, not a bundled upstream-server catalog. Other users do not need a
new MCPace build when they choose different servers. They install whatever
upstream MCP packages or binaries they trust, then add those entries to their
own `mcp_settings.json`, per-server fragments in `mcp_settings.d/*.json`, additional files/directories listed in `mcpace.config.json` `mcpSettings.includePaths` / `mcpSettings.includeDirs`, files listed in `MCPACE_MCP_SETTINGS`, or directories listed in `MCPACE_MCP_SETTINGS_DIRS`.

The easiest top-down command is the read-only wiring guide:

```bash
mcpace connect --json
mcpace connect cursor-local --server filesystem
```

It resolves the configured MCPace endpoint, selected client target, upstream source inventory, readiness blockers, and exact next commands.

The easiest server-management path is now preset-first:

```bash
mcpace server presets
mcpace server install filesystem --path . --dry-run
mcpace server install filesystem --path .
mcpace server sources --json
mcpace server test filesystem --refresh
mcpace server disable filesystem --dry-run
mcpace server enable filesystem --dry-run
mcpace server remove filesystem --dry-run
```

`server presets` reads the merged preset catalog from `mcpace.config.json` `mcpPresets.includePaths` plus `MCPACE_MCP_PRESETS`; the packaged default is `presets/mcp-servers.json`. Useful starter servers are data-driven and editable instead of compiled into Rust code. `server starter` installs the conservative default starter pack; today that is only `filesystem` with an explicit allowed path. `server install <preset>` writes a single-server JSON fragment under `mcp_settings.d/` using preset data, while `server add` remains the fully custom escape hatch. `server test` runs a live stdio `tools/list` smoke against the configured server, `server disable` / `server enable` pause and resume a server without deleting the entry, and `server remove` deletes the matching entry from the source file where MCPace found it. Use `--dry-run` to preview, `--force` to replace an existing fragment, `--settings <path>` to target a specific source file, and `--url <url> --type streamable-http` to inventory a remote HTTP MCP server for the future HTTP-upstream lane. URL entries are limited to `http://` or `https://` endpoints so future remote forwarding does not inherit arbitrary URI schemes.

You can still add user-supplied servers explicitly in JSON. MCPace accepts ordinary stdio-style MCP entries without also requiring a hardcoded server declaration in `mcpace.config.json`:

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
- new server names are discovered from root `mcp_settings.json`, configured
  `mcpSettings.includePaths`, `mcpSettings.includeDirs`, `mcp_settings.d/*.json`,
  `MCPACE_MCP_SETTINGS`, or `MCPACE_MCP_SETTINGS_DIRS` at runtime, without
  recompiling MCPace;
- `mcpace server list --json`, `upstream_probe`, `upstream_catalog`,
  `upstream_tools`, `upstream_call`, and `upstream_batch` operate on those
  configured names;
- MCPace does not silently install arbitrary upstream packages. Preset installs write reviewable MCP settings fragments; runtime package execution remains explicit through commands such as `npx -y ...`, and `server test` surfaces failures before clients use the server.

Stdio upstream processes do not inherit the full MCPace parent environment. MCPace clears the child environment, adds a small process-launch baseline, then applies explicit `env` values and allowlisted local `env_vars` values. `env_vars` accepts either string names or local object entries such as `{ "name": "TOKEN", "source": "local" }`. Add secrets explicitly; do not rely on accidental parent-env inheritance.
Runtime cache fingerprints include env variable names and hashed values only, not plaintext secret values.
When upstream startup, timeout, or JSON-RPC errors include stderr, MCPace keeps bounded diagnostic context but redacts likely tokens, passwords, credentials, API keys, private keys, Authorization values, and bearer tokens before surfacing the error.

HTTP/Streamable HTTP entries are inventoried and reported, but the current stdio bridge only forwards callable stdio upstreams. HTTP upstream fan-out remains blocked honestly in diagnostics until implemented.

## Configure the MCPace endpoint advertised to clients

The compatibility default is still:

```text
http://127.0.0.1:39022/mcp
```

For different local ports, tunnels, relays, or cloud/API client surfaces, configure the advertised endpoint instead of editing client templates by hand:

```json
{
  "serve": {
    "host": "127.0.0.1",
    "port": 39022,
    "mcpPath": "/mcp",
    "publicUrl": "https://your-relay.example/mcp"
  }
}
```

Environment overrides are also supported: `MCPACE_SERVE_HOST`, `MCPACE_SERVE_PORT`, `MCPACE_SERVE_PATH`, and `MCPACE_PUBLIC_MCP_URL`. `client install` and `client export` use this resolver, so different clients do not have to share a compiled-in URL.

During Streamable HTTP `initialize`, MCPace now returns `Mcp-Session-Id` and `MCP-Protocol-Version` response headers. Upstream lease affinity also recognizes common client/chat/project-root headers so different clients, chats, and workspaces can stay separable while the durable HTTP session store remains future work.

## Useful commands

```bash
npm test
npm run inventory:source
npm run inventory:project
npm run verify:boot
npm run verify:install-readiness
npm run verify:product-practice
npm run verify:runtime-trace
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
- `mcpace server presets` lists editable useful MCP starter presets.
- `mcpace server starter --path .` installs the conservative local developer starter pack.
- `mcpace server install filesystem --path .` installs a useful preset without memorizing package args; `context7`, `git`, and `playwright` are opt-in presets for docs, repository context, and browser automation.
- `mcpace server add <name> --command <cmd> [--arg <arg>...]` writes a fully custom per-server MCP settings fragment.
- `mcpace server enable|disable <name> [--dry-run]` pauses or resumes a server entry without deleting JSON.
- `mcpace server remove <name> [--dry-run]` removes a server entry from the source file where it was found.
- `mcpace server sources --json` inventories every MCP settings source and duplicate/override warning.
- `mcpace server test <name> --refresh --json` runs a live stdio `tools/list` smoke before wiring a client.
- `mcpace server list --json` shows both policy-declared servers and source-only MCP settings servers.
- `mcpace server capabilities --json --name <server>` shows transport, command, source, and policy details.
- `mcpace verify readiness --json` reports missing stdio commands for source-only servers.
- `mcpace lab report --json` writes a lab report.
- `mcpace release build --json` creates local release artifacts and does not publish.

## Not implemented yet

Network publication, HTTP upstream fan-out from the stdio bridge, durable HTTP session enforcement, cloud relay support as a product lane, and enterprise/team policy management are not implemented yet. Keep those claims out of product docs until they have proof artifacts.
