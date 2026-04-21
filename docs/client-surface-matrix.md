# Client Surface Matrix

Do **not** model compatibility by brand name alone.

A single vendor can expose multiple MCP surfaces with different transport, auth, and feature limits. MCPace should therefore reason about **surfaces** instead of pretending that one `cursor` or one `claude` target describes everything.

## Why surfaces matter

Examples already present in current official docs:

- **Claude Code** is a local/project/user-scoped MCP host.
- **Claude API MCP connector** is a cloud/API surface that only supports tools and only reaches public HTTP MCP servers.
- **Cursor local editor/CLI** share one config, but **Cursor cloud agents** are a separate surface with different transport guidance.
- **GitHub Copilot CLI** and **GitHub Copilot cloud agent** differ on resources/prompts and remote OAuth.

The runtime and the lab should therefore key on:

- `familyId` — vendor/product family (`claude`, `cursor`, `github-copilot`)
- `surfaceClass` — `local`, `cloud`, or `generic`
- `surfaceKind` — more precise operational shape like `local-cli`, `cloud-agent`, or `cloud-api-connector`

## Current surface-aware catalog

The Rust catalog now carries documented surfaces for:

- `codex`
- `claude-code`
- `claude-api-connector`
- `cursor-local`
- `cursor-cloud-agents`
- `kiro-ide`
- `kiro-cli`
- `windsurf`
- `gemini-cli`
- `github-copilot-cli`
- `github-copilot-cloud-agent`
- `hermes-agent`
- `generic-stdio`
- `generic-http`
- `public-http-connector`

## Minimum fields each surface should expose

- `id`
- `familyId`
- `surfaceClass`
- `surfaceKind`
- `configFormat`
- `configPaths`
- `configPrecedence`
- `nativeScopes`
- `supportedIngresses`
- `documentedFeatures`
- `documentedConstraints`
- `notes`

## Constraint examples that matter to MCPace

- `tools-only`
- `public-http-only`
- `no-remote-oauth`
- `tool-budget-100`
- `sse-deprecated`
- `shared-config-cli-ide`
- `agent-overrides-workspace-user`

## What the runtime should do with these constraints

- `tools-only` — never promise resources/prompts on that surface
- `public-http-only` — require relay/public HTTP instead of a local stdio launcher
- `no-remote-oauth` — warn that OAuth-based remote MCP needs another surface or another auth path
- `tool-budget-*` — budget tool exposure during install/export instead of enabling everything
- `shared-config-*` — patch only the MCPace-owned block and preserve user-managed entries

## What is still missing

Surface metadata is now catalogued and visible through `mcpace client list`, but the following are still planned:

- live install/export patchers per surface
- cloud/public relay path
- real-client compatibility traces
- tool-budget-aware export rules
- tools-only feature gating in the runtime itself
