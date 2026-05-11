# Adjacent Tool Landscape

This document captures what publicly visible neighboring tools do well, and what MCPace should copy **carefully** instead of cargo-culting blindly.

## Scope and evidence rules

- Use only public docs, public repos, and public install surfaces.
- Closed or hosted tools often do **not** publish their full internal implementation stack.
- When a stack detail is not public, mark it **NOT CONFIRMED** and infer only from visible package/install/repo surfaces.
- Copy durable product patterns, not feature checklists.

## Compared tools

| Tool | Public surfaces | Public stack signals | Patterns worth borrowing | What not to copy blindly |
|---|---|---|---|---|
| **OpenAI Codex** | terminal CLI, IDE, desktop app, web/cloud tasks | open-source repo is Rust-heavy, with `codex-cli`, `codex-rs`, `sdk`, `package.json`, `pnpm-workspace.yaml`; distributes via npm, Homebrew, and platform binaries | one engine across many surfaces; explicit local vs cloud split; worktree/background-task model; MCP support and skills as reusable capability layer | do not make cloud/background execution phase 1; do not treat app/web breadth as a prerequisite for a good local hub |
| **Claude Code** | terminal, IDE, desktop, web | public docs show one engine across surfaces; public Agent SDK is Python + TypeScript; internal CLI/runtime implementation stack is **NOT CONFIRMED** publicly | same engine across multiple surfaces; remote session handoff; managed cloud lane as a separate concern; permission model matters | do not guess Anthropic’s internal stack; do not copy closed-product breadth before core proof |
| **OpenCode (current)** | terminal/TUI, desktop beta, remote-driving client/server model | current open-source repo is TypeScript-heavy with `package.json`, `turbo.json`, bun/Nix tooling; public docs explicitly say it has a client/server architecture | treat TUI as one client, not the product core; keep provider-agnostic design; make install/distribution easy | do not make TUI-first architecture the main priority before core runtime exists |
| **OpenCode (archived historical repo)** | terminal/TUI | archived older repo was Go-based and used Bubble Tea + SQLite | proof that lightweight local state and terminal-first UX can stay simple | history is useful, but it is no longer the active OpenCode direction |
| **Goose** | CLI, server, desktop app, API, MCP/ACP extensions | public repo is a Rust workspace with `goose-cli`, `goose-server`, `goose-mcp`, Electron UI, and evals; maintainers explicitly discuss converging multiple binaries toward one protocol | workspace split by responsibility; MCP extension layer; evals close to the product; one-protocol direction is aligned with MCPace’s simplification goal | do not add a desktop shell or extra binaries too early |
| **Hermes Agent** | CLI plus many channels, automations, optional MCP | public repo is Python-first with `pyproject.toml`, `uv.lock`, and a small JS surface; docs show `uv` install, config under `~/.hermes`, and optional MCP extras | single gateway/control-plane mindset; config/state outside git; scheduled automations as a later capability | do not explode MCPace into a multi-channel assistant product |
| **OpenClaw** | local-first gateway, CLI, web/control surfaces, many channels | public repo is TypeScript-heavy with `package.json`, `pnpm-*`, `pyproject.toml`, Docker artifacts, native/mobile shells; docs/repo frame the gateway as the control plane | strong control-plane mental model; local-first gateway; explicit operations and sandboxing concerns | do not copy the huge channel surface; it is a different product category |

## Repeated product patterns across strong tools

### 1. One durable core, many surfaces

The strongest tools are not “many unrelated entrypoints”.
They usually have one engine/core and then expose it through CLI, desktop, IDE, web, or background-task surfaces.

### 2. Control plane != presentation layer

Terminal UI, desktop UI, web UI, and chat surfaces are clients.
The control plane owns config, state, routing, sessions, health, logs, and policy.

### 3. Local state matters

The better tools keep durable user/runtime state outside git and treat it as first-class product state.
That includes sessions, project registries, work queues, logs, and per-client install/export state.

### 4. Cloud/background lanes are separate, not mandatory

Codex cloud, Claude Code web, and automations are strong patterns, but they are all **secondary** to having a working local core.

### 5. Adapters beat giant product sprawl

Good tools stay small by having adapters/connectors/extensions instead of baking every integration into the core runtime.

### 6. Security and isolation are not optional

Transport boundaries, local-only bind defaults, permissioning, state isolation, and clean install/update flows are visible in the stronger products.

## What MCPace should copy

1. **One public binary and one command taxonomy**.
2. **A real local hub/control-plane process**.
3. **Thin client adapters** rather than per-client one-off logic everywhere.
4. **Config/state outside git** with explicit schema validation.
5. **Local state and logs** as first-class product data.
6. **Optional remote/background lane later**, not as day-one scope.

## What MCPace should not copy

1. a feature-checklist race against Codex / Claude Code / OpenCode;
2. a GUI-first rewrite;
3. a cloud-first rewrite;
4. a many-binary or many-primary-entrypoint product;
5. a product scope that tries to become a universal multi-channel assistant.

## Conclusion for MCPace

The right target is still:

- a **Rust** core;
- a **single local hub**;
- a **CLI-first** public surface;
- a **local control plane** with typed config and durable state;
- **client adapters** and **connector adapters** around that core;
- optional desktop/web/cloud layers only after local proof exists.

## Public sources used

- MCP specification: <https://modelcontextprotocol.io/specification>
- Official Rust MCP SDK (`rmcp`): <https://github.com/modelcontextprotocol/rust-sdk>
- OpenAI Codex repo and docs: <https://github.com/openai/codex> and <https://developers.openai.com/codex>
- Claude Code docs: <https://code.claude.com/docs/en/overview>
- OpenCode repo/site: <https://github.com/anomalyco/opencode> and <https://opencode.ai>
- Goose docs/repo: <https://goose-docs.ai/> and <https://github.com/aaif-goose/goose>
- Hermes Agent docs/repo: <https://hermes-agent.nousresearch.com/> and <https://github.com/NousResearch/hermes-agent>
- OpenClaw repo/docs: <https://github.com/openclaw/openclaw> and <https://docs.openclaw.ai>

