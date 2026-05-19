# MCP overhead and optimization guardrails

MCPace must prove that automatic MCP server support does not turn into hidden overhead or random side effects. This document defines the source-level guardrail added for the auto-install/evidence-first model.

## What is measured

`npm run verify:overhead-pressure` runs `scripts/mcp-overhead-pressure-audit.mjs`. It is synthetic and local-only. It does not download packages, start MCP servers, send `initialize`, or call `tools/call`.

The audit measures:

- evidence-profile inference for many server specs;
- `mcp_settings.d/*.json` fragment scan and JSON parse pressure;
- scheduler route-key decisions across clients, chats, sessions, projects, repos, db paths, credentials, tenants, browser contexts, and remote transport sessions;
- heap delta while holding a bounded profile set;
- safety invariants proving that unknown/high-risk profiles stay review-gated.

Default release check:

```bash
npm run verify:overhead-pressure
```

Larger local benchmark:

```bash
npm run benchmark:overhead-pressure
```

The benchmark is intentionally not a default release gate because host CPU, mirrors, and sandbox limits vary. The release gate records the observed numbers and fails only when the measured source-level overhead exceeds conservative budgets.

## Why this exists

Automatic MCP discovery can become expensive in three ways:

1. repeatedly parsing large settings fragments;
2. reclassifying the same command/URL/policy on every UI refresh or chat switch;
3. starting or probing servers just to render a dashboard or decide whether a tool may exist.

The correct default is metadata-only planning. Server process start and live `tools/list` probing belong to explicit `server test`, reviewed enablement, or a real client/tool path.

## Optimization rules

- Cache profile decisions by normalized command, URL, args, policy, and settings-fragment mtime/hash.
- Keep dashboard/client listing metadata-only unless the operator requests refresh/test.
- Start stdio servers lazily; do not start all configured servers on app open.
- Treat remote Streamable HTTP as session-bound until explicit stateless evidence exists.
- Use route keys instead of global singletons: project/repo/db/session/credential/browser/provider locks allow safe concurrency without cross-chat leakage.
- Keep package-manager work out of normal runtime. Package install/lock/tarball survey belongs to explicit lab commands and must run with scripts disabled.

## What this does not prove

This source-level audit does not prove final Rust binary latency, OS-specific process spawn cost, real network latency, or correctness of every third-party MCP server. Before a public native release, run Rust build/tests plus host-specific latency/memory baselines.
