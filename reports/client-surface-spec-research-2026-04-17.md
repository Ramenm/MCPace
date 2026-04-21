# Client surface research snapshot — 2026-04-17

This is a repo-internal synthesis of the client-surface assumptions already encoded in `src/client_catalog.rs` and `docs/client-surface-matrix.md`.

It is **not** a claim that every surface below was exercised live in this repo.

## Why this exists

MCPace needs to plan by **surface**, not by brand name alone.

The current repo already distinguishes:

- local CLI / IDE surfaces
- cloud-agent surfaces
- cloud/API connector surfaces
- generic stdio / generic HTTP hosts

## Surface classes currently modeled

| Surface | Class | Kind | Key documented constraints |
|---|---|---|---|
| `codex` | local | `local-cli-ide` | `shared-config-cli-ide` |
| `claude-code` | local | `local-cli-ide-browser` | `managed-config`, `sse-deprecated` |
| `claude-api-connector` | cloud | `cloud-api-connector` | `tools-only`, `public-http-only`, `beta-header-required` |
| `cursor-local` | local | `local-editor-cli` | `shared-config-cli-editor`, `fixed-oauth-callback` |
| `cursor-cloud-agents` | cloud | `cloud-agent` | `cloud-vm`, `no-sse`, `http-preferred` |
| `kiro-ide` | local | `local-ide` | `workspace-overrides-user` |
| `kiro-cli` | local | `local-cli` | `agent-overrides-workspace-user`, `tool-name-rules` |
| `windsurf` | local | `local-ide` | `tool-budget-100` |
| `gemini-cli` | local | `local-cli` | `settings-json` |
| `github-copilot-cli` | local | `local-cli` | `built-in-github-server`, `session-additional-config` |
| `github-copilot-cloud-agent` | cloud | `cloud-agent` | `tools-only`, `no-remote-oauth`, `repo-level-config` |
| `hermes-agent` | local | `local-agent` | `config-yaml-plus-env`, `oauth-pkce`, `capability-aware-resource-prompt-wrapper` |
| `generic-stdio` | generic | `generic-stdio-host` | `host-defined` |
| `generic-http` | generic | `generic-http-host` | `host-defined` |
| `public-http-connector` | cloud | `generic-public-http-connector` | `tools-only`, `public-http-only` |

## Practical consequences already visible in the repo

- tools-only surfaces should not be described as if they support resources/prompts
- public-HTTP-only surfaces are not satisfied by a local stdio-only lane
- tool-budget surfaces need export budgeting later instead of “enable everything”
- local vs cloud differences should stay explicit in the runtime lab and compatibility work

## What is still missing

- sanitized real-host traces for each major surface
- compatibility proof on supported hosts
- runtime enforcement for tools-only and budget constraints
