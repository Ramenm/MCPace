# Client Metadata Routing Contract

This document defines metadata-first routing for `mcpace client plan` and the
future local hub runtime.

## Goals

- avoid hardcoded client name tables;
- prefer explicit machine-readable metadata over terminal/process heuristics;
- keep project routing sticky by `session.id` when metadata is available;
- keep one future entry point for many clients.

## Metadata envelope

`MCPACE_CLIENT_METADATA_JSON` can provide a JSON object with these optional
fields:

- `client.id` (or `clientId`): stable client identity;
- `session.id` (or `sessionId`): stable session identity;
- `workspaceRoots` / `workspace.roots` / `workspaces` / `roots`: candidate project roots;
- `cwd` or `_meta["com.mcpace/context"].cwd`: current working directory hint;
- `conversationId`, `clientInstanceId`, `transportSessionId`: optional routing hints;
- `credentialProfileId`: optional credential-partition hint for `credential-scoped` servers;
- `transport` / `ingress` / `transportPreference`: requested client ingress.

Example:

```json
{
  "client": { "id": "codex" },
  "session": { "id": "sess_2026_04_15_a1" },
  "workspaceRoots": ["/work/project-a"],
  "transport": "stdio"
}
```

Explicit env overrides remain supported:

- `MCPACE_CLIENT_ID`
- `MCPACE_SESSION_ID`
- `MCPACE_PROJECT_ROOT`
- `MCPACE_CLIENT_TRANSPORT`
- `MCPACE_CLIENT_METADATA_JSON`

## Resolution order

### Client identity

1. explicit `--client-id`;
2. explicit `MCPACE_CLIENT_ID`;
3. metadata `client.id` / `clientId`;
4. fallback `unknown-client`.

### Session identity

1. explicit `--session-id`;
2. explicit `MCPACE_SESSION_ID`;
3. metadata `session.id` / `sessionId`;
4. unresolved.

### Project selection

1. explicit `--project-root`;
2. explicit `MCPACE_PROJECT_ROOT`;
3. metadata single root from `workspaceRoots` / `workspace.roots` / `workspaces` / `roots`;
4. metadata roots + `cwd` if the cwd selects a unique root;
5. metadata `cwd` as a weak fallback when no roots exist;
6. unresolved.

### Ingress preference

1. explicit `--transport` / `--ingress`;
2. explicit `MCPACE_CLIENT_TRANSPORT` / `MCPACE_CLIENT_INGRESS`;
3. metadata transport field;
4. fallback `stdio` for the current generic local-client planning path.

## Session bindings

The future hub should derive one session binding key from:

- `clientId`
- an always-present `sessionLeaseId` (external session id when present, otherwise a derived internal lease)
- `projectRoot` when project-local routing matters

Credential-partitioned servers should additionally resolve a `credentialProfileId` when available instead of falling back immediately to coarse bindings like `oauth` or `api-key`.

Example binding key:

```text
client:codex|session:external:sess_2026_04_15_a1|project:/work/project-a
```

## Concurrency policy handoff

Client metadata must not decide server safety directly.
It only provides routing context.
Server safety stays policy-driven through server metadata:

- `scopeClass`
- `concurrencyPolicy`
- `stateBinding`
- `credentialBinding`

The client plan command should therefore answer two different questions:

1. who is this client/session/project?
2. how should each server be isolated or serialized for that context?


## MCP-native hints now parsed

`mcpace client plan` now accepts richer metadata shapes, including:

- `params.clientInfo.name` as a fallback client identity;
- `roots` entries using MCP-style `{ "uri": "file:///..." }` objects;
- `_meta["com.mcpace/context"]` for optional `cwd`, `conversationId`, `clientInstanceId`, `transportSessionId`, and `credentialProfileId` hints.
