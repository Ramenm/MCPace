# ADR 0004: Source-only MCP configuration and stdio environment isolation

## Context

MCPace now ships without packaged upstream MCP server defaults. Users add upstream servers explicitly in `mcp_settings.json`. Standard stdio MCP configuration uses `command`, `args`, `env`, `env_vars`, and `cwd`; MCPace also inventories HTTP-like `url` entries while stdio forwarding remains the implemented call path.

## Problem / goal

The hub must support arbitrary user-supplied stdio MCP servers without reintroducing hardcoded server catalogs and without leaking the host process environment into untrusted child processes. A local MCP server runs as a child process and can read any environment variable inherited from the parent, so implicit inheritance is a credentials risk.

## Constraints and non-goals

- Do not bundle or recommend upstream MCP servers by default.
- Keep `mcp_settings.json` as the source of truth for source-only upstream servers.
- Preserve compatibility for ordinary stdio launch basics such as `PATH`, temporary directories, and explicit `env` / `env_vars` values.
- Do not implement HTTP upstream fan-out in this ADR; HTTP entries remain inventory/diagnostic-only until a separate adapter is implemented.
- Do not silently mutate `mcpace.config.json` from source-only settings.

## Considered options

1. **Inherit all parent environment variables.** Lowest compatibility risk, highest credentials risk. Rejected because `env_vars` would not be a real allowlist.
2. **Clear the child environment and pass only explicit `env` / `env_vars`.** Strongest isolation, but breaks common stdio launchers that rely on basic process variables such as `PATH`.
3. **Clear the child environment, add a minimal non-secret process baseline, then apply explicit `env` / `env_vars`.** Chosen. It limits accidental secret propagation while preserving common launcher behavior.

## Selected solution

`spawn_stdio_server` calls `env_clear()`, adds a small platform baseline for process execution, sets MCPace runtime variables, and then applies explicitly configured `env` plus allowlisted local `env_vars`. `env_vars` now accepts both string names and Codex-shaped local object entries such as `{ "name": "TOKEN", "source": "local" }`. Remote env sources are skipped because this local runtime has no remote executor environment.
Session/tool-cache fingerprints retain env variable names but hash explicit env values instead of embedding plaintext secrets.

## Consequences / risks

- Servers that accidentally depended on implicit secret inheritance now need explicit `env` or `env_vars` entries.
- The minimal baseline still forwards non-secret process context such as `PATH` and home/temp paths. This is a compatibility trade-off, not a secret boundary.
- HTTP upstream entries are still visible in inventory but not callable through stdio forwarding.

## Plan / verification

- Keep packaged `mcp_settings.json.mcpServers`, `mcpace.config.json.servers`, and `server-candidates.json` empty.
- Validate source-only stdio config and command prerequisite reporting through Rust tests.
- Guard packaged defaults and environment isolation with `tests/node/mcp-config-contract.test.js`.
- Add an eval fixture for the adversarial case where compatibility is used to justify forwarding the whole parent environment.

## Open questions

- Whether to expose an explicit, documented `inheritEnv` escape hatch for trusted local-only setups. Current recommendation: do not add it until there is a concrete production compatibility case and an auditable warning path.
- Whether HTTP upstream fan-out should live in `upstream.rs` or a separate transport adapter module. Current recommendation: keep it separate once implemented.
