# ADR 0002 — Project-aware client launcher instances

## Status

Accepted for launcher/runtime layer.

## Context

The long-term target for MCPace is still a single visible endpoint with internal project routing, registry, and project-local stateful tools. But this repository only contains the launcher/manager layer, not MCPace hub internals. We still need a safe way to stop mixing cwd-sensitive tools across unrelated projects today.

## Decision

Implement project-aware launcher-managed instances as the near-term safe architecture:

- keep `mcpace.cmd` / `mcpace.sh` as the only client-facing command path;
- detect project roots from the client working directory or explicit env overrides;
- when the client is outside the manager root, create/reuse an isolated instance with:
  - dedicated state root,
  - dedicated hub container name,
  - dedicated hub port,
  - primary workspace bound to the detected project root;
- forward the client working directory explicitly and derive a best-effort client identity for per-client instance keys;
- share auth material from the manager root so instances do not drift into incompatible bearer tokens or admin credentials;
- prune stale instance records when their backing project roots disappear;
- preserve legacy/shared mode when the client is working inside the manager root or when auto-detection is not safe.

## Consequences

### Good

- `filesystem`, `serena`, and similar cwd-sensitive tools stop sharing the same primary workspace across unrelated projects.
- Adding a new client/project becomes near-zero-touch: start from that project and the launcher does the rest.
- The public client contract stays one launcher path instead of many manually managed endpoint configs.

### Trade-offs

- This is still manager-side orchestration, not hub-native routing.
- Multiple project instances mean more Docker containers and ports while those projects are active.
- Idle instance eviction is a follow-up concern, not solved in this ADR.

## Follow-up

- add registry cleanup / idle eviction;
- surface instance inventory more deeply in dashboard/start UI;
- keep driving toward hub-native routing once MCPace server internals are available in source.
