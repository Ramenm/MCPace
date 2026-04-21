# Server Segmentation and Arbitration Model

This page describes how MCPace should separate MCP servers into safe policy
classes and how the future hub should avoid stream contention across clients.
It matches the current runtime policy model already present in
`mcpace.config.json`:

- `kind`
- `scopeClass`
- `concurrencyPolicy`
- `required`

## Why this split helps

A single flat server list hides risk.
Some servers are always-safe defaults, some depend on desktop state, and some do
not tolerate parallel traffic.
The hub should therefore own arbitration instead of letting clients collide.

## Runtime kinds

- `container-stdio`: process is managed in the local runtime.
- `host-bridge`: process is on the host and reached through a bridge.
- `external-http`: bridge to an external service endpoint.
- `remote-http`: direct remote endpoint.

## State scope classes

- `shared-global`: one state for all sessions.
- `project-local`: state must follow project context.
- `credential-scoped`: state follows auth profile.
- `shared-exclusive`: shared resource, but single active owner.

## Concurrency policies

- `multi-reader`: safe for parallel requests.
- `isolated-per-project`: parallel only if project-isolated.
- `single-writer`: serialize requests per process instance.
- `single-session`: one active session lease at a time.

## MCPace arbitration rules

These are MCPace hub rules, not protocol guarantees from upstream servers.

### Rule 1 — hub-owned stdio

If a server uses `stdio`, MCPace should own the child process.
Clients should not write to the same stdio stream directly once the hub exists.

### Rule 2 — process scope follows `scopeClass`

- `shared-global` -> one shared process partition
- `project-local` -> one partition per project root
- `credential-scoped` -> one partition per credential scope
- `shared-exclusive` -> one exclusive partition

### Rule 3 — request serialization follows `concurrencyPolicy`

- `multi-reader` -> no mutex required
- `isolated-per-project` -> mutex per project partition
- `single-writer` -> mutex per process instance
- `single-session` -> exclusive global lease per server

### Rule 4 — unresolved project roots are not harmless

If a server is `project-local` or `isolated-per-project` and the client context
has no project root, the hub should not silently share that server across
sessions.
It should either pause routing, force isolation, or return a clear warning.

## What `mcpace client plan` shows today

The current native `client plan` command computes a read-only plan for:

- client/session/project identity resolution;
- single-entry-point binding key;
- per-server process scope key;
- per-server request strategy;
- warnings for unsafe sharing conditions.

This is planning logic only.
It is not yet a live hub lifecycle implementation.
