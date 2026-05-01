# Greenfield single-hub architecture

## Decision frame

The next useful simplification is not “port one more legacy script”.
It is to define the **end-state product** clearly:

- one binary: `mcpace`
- one local control/runtime process: `mcpace hub`
- one user config model
- one state root outside git
- one installer/export path for supported clients
- one compatibility bridge while cutover is incomplete

That makes the rewrite about **product shape**, not about preserving the current top-level script inventory.

## Product thesis

`MCPace` should become a **single local MCP hub for many clients**.

The user story should be:

1. install `mcpace`
2. run `mcpace init`
3. run `mcpace client install <client>`
4. use the client normally
5. inspect health with `mcpace doctor` and `mcpace status`

Users should not need to understand MCP, container wiring, per-client settings drift, or repo-local contributor tooling.

## Why a single hub is stronger than many scripts

The current repo still mixes three concerns:

- operator commands
- runtime orchestration
- legacy shell compatibility

That inflates public surface area even when the underlying runtime intent is simple.

A single local hub reduces that sprawl by separating:

- **CLI surface** — what operators type
- **hub runtime** — what stays running and owns state
- **client adapters** — what is written into Cursor / VS Code / Claude Desktop / generic configs
- **upstream connectors** — how local stdio, local HTTP, remote HTTP, and host bridges are managed

## Target architecture

### 1. CLI plane

`mcpace` becomes a subcommand-based CLI, not a pile of top-level scripts.

Target command groups:

- `mcpace init`
- `mcpace hub up|down|status|logs`
- `mcpace client install|export|list`
- `mcpace server list|show|enable|disable|test|capabilities`
- `mcpace profile list|set`
- `mcpace project list|scan`
- `mcpace verify doctor|check|smoke|readiness|probe`
- `mcpace repair`
- `mcpace package build`

### 2. Hub runtime plane

The runtime process owns the truth that should not be recomputed ad hoc by every wrapper:

- resolved config
- effective profile
- server registry
- project registry
- session routing
- connector health
- logs and recent failures
- compatibility decisions per client/server pair

### 3. Client adapter plane

Each client installer/exporter should be thin.

The adapter only needs to answer:

- does this client prefer stdio or local HTTP?
- where should the launcher/config file be written?
- which auth/token pattern does this client need?
- what capability limitations must be declared?

### 4. Upstream connector plane

Every server should be represented by one connector contract:

- `local-stdio`
- `local-http`
- `remote-http`
- `host-bridge`

The hub should hide the transport details and expose one stable operator story.

## Runtime data model

### Config

The target config should move toward one explicit hub schema.
This pass introduces `schemas/mcpace-hub.schema.json` plus worked examples under `examples/`.

Key concepts:

- runtime mode and ingress endpoints
- supported clients
- profiles
- server registry
- project discovery roots
- compatibility bridge flags

### State

The target state should live outside git and outside source templates:

- runtime sessions
- project registry
- generated client exports
- auth/token references
- rolling logs
- probe and readiness snapshots

## Ingress model

The hub should support both standard MCP entry shapes:

- generated stdio launchers for clients that prefer a process entrypoint
- local Streamable HTTP for clients that support HTTP cleanly

The goal is **one product**, not one transport.

## Scenario coverage

The target hub must handle at least these user scenarios.

### Typical

- one developer, one laptop, one client
- one developer, multiple clients on the same machine
- safe default profile with only required integrations
- project-local filesystem and index-aware servers

### Edge

- Docker unavailable
- Node present but a required package missing
- remote credential-backed server enabled without credentials
- workspace roots changed while a client session stays open
- host bridge only available on one platform

### Adversarial / failure

- operator asks for full support without proof
- partial migration leaves docs claiming more than the runtime does
- client export writes stale or repo-local state
- a local bridge fails and is incorrectly reported as healthy

## Non-goals

- no multi-tenant hosted SaaS in the first greenfield cut
- no UI-first rewrite before the command model stabilizes
- no hidden runtime state in git
- no requirement for `skills/`, OMX, or contributor automation in the shipped path

## Recommended implementation order

1. freeze command model and config schema
2. split workspace around CLI / core / hub / adapters / compat
3. implement config/state loading
4. implement local hub runtime and health store
5. implement stdio launcher path
6. implement local Streamable HTTP ingress
7. implement server registry and profile engine
8. implement client installers/exports
9. port verification
10. cut over legacy scripts only after host proof
