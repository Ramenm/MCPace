# MCPace Single-Hub Stack Blueprint

This is the narrowed target stack for MCPace after comparing adjacent tools.
It is intentionally smaller than “everything those tools have”.

## Product shape

MCPace should be:

- one public binary: `mcpace`
- one local control plane / hub runtime
- one typed config/state contract
- one small compatibility alias layer while cutover is incomplete
- many thin client and connector adapters around the core

It should **not** begin as:

- a desktop-first app
- a TypeScript reimplementation of the runtime
- a cloud-first agent
- a feature clone of Codex / Claude Code / OpenCode
- a multi-channel assistant product

## Chosen stack by layer

| Layer | Choice | Why this is the default |
|---|---|---|
| Product language | Rust | native binary distribution, strong error model, fits process/config/runtime orchestration |
| MCP protocol layer | `rmcp` | official Rust MCP SDK; avoids inventing protocol glue from scratch |
| Async runtime | `tokio` | needed once the hub becomes a real process and uses MCP transports, subprocesses, timers, or concurrent checks |
| CLI taxonomy | `clap` | durable subcommand model with strong help/validation |
| Config serialization | typed Rust models + schema-backed loaders | current repo uses a small in-repo JSON layer for offline Linux proof; richer typed loading comes next |
| Schema generation | `schemars` | keeps Rust types and JSON Schema close instead of drifting manually |
| Schema validation | `jsonschema` | validates config/examples at load time and in tests |
| App/domain errors | `anyhow`, `thiserror` | clean split between top-level app context and typed domain errors |
| Local state store | `rusqlite` | lightweight, single-file, good fit for a local control plane and small metadata/state needs |
| Local HTTP / admin plane | `axum`, `tower` | only for localhost control APIs and local Streamable HTTP ingress |
| npm distribution lane | npm workspace launcher + future platform packages | gives npm users a familiar install/update path without changing the implementation core |
| npm wrapper language | plain Node ESM first | a compile-free launcher is easier to verify than starting with TypeScript |
| Structured diagnostics | `tracing`, `tracing-subscriber`, `tracing-appender` | rolling logs, structured events, easier verification and supportability |
| File/config watch | `notify` | only if hot reload or config watching proves useful |
| CLI integration tests | std `Command` today, `assert_cmd` later if needed | current Linux proof stays offline; richer helpers can come back later if they earn their cost |
| Rust test runner | `cargo-nextest` later | consistent repo-level test policy once the core surface is larger |
| Dependency governance | `cargo-deny`, `cargo-audit` later | license/advisory/duplication governance once dependency surface grows again |
| Cross-build helper | `cross` later | useful for packaging, but not a substitute for real host proof |

## Why SQLite is the initial state choice

A single local hub needs small durable state before it needs a service database.
MCPace needs to remember things like:

- session registry
- client install/export state
- project roots and routing hints
- health snapshots
- background task metadata later
- log indexes / pointers if needed

`rusqlite` keeps that state local, explicit, and lightweight.
It avoids adding a service dependency or a heavier ORM before the product earns it.

## Why the control plane should stay local first

The strongest comparable tools add cloud/background execution **after** a strong local core exists.
For MCPace, phase 1 should stay local-first:

- local state root
- localhost-only admin/control plane
- local stdio launchers
- local Streamable HTTP ingress
- client adapter generation/export

Only later should MCPace consider:

- remote workers
- background tasks
- shared cloud execution
- web/mobile control surfaces

## Explicitly deferred

### Deferred product layers

- desktop shell (`Tauri` or `Electron`)
- browser control UI beyond a thin local diagnostics page
- hosted cloud task runner
- mobile surfaces
- multi-channel/chat surfaces

### Deferred technical layers

- heavy ORM / migrations framework
- search index / vector store inside the hub core
- plugin marketplace
- automatic remote sync

## Workspace target

The next stable workspace split should be:

- `mcpace-cli` — CLI entry and command taxonomy
- `mcpace-core` — config models, policies, identifiers, shared domain logic
- `mcpace-hub` — runtime/control plane, sessions, health, state
- `mcpace-adapters` — client installers/exporters and connector adapters
- `mcpace-compat-aliases` — small explicit compatibility routing only for real grouped Rust surfaces

## Implementation order

1. workspace split
2. typed config + schema generation/validation
3. command taxonomy via `clap`
4. local state root + SQLite metadata store + structured logs
5. local hub runtime
6. stdio + local Streamable HTTP ingress
7. client install/export flows
8. verification and readiness beyond read-path inspection
9. only then: optional UI or remote/background lane

## Guardrails

- no claim of parity without host proof
- no cloud-first expansion before local core proof
- no feature cloning from adjacent tools without product-level justification
- no merging of `skills/` into end-user runtime prerequisites
- no second repo until crate boundaries and cutover are stable

## Deferred npm decisions

- TypeScript build chain for the npm launcher until the launcher genuinely needs compilation or shared typed code.
- postinstall binary downloaders; prefer release artifacts and explicit install lanes.
