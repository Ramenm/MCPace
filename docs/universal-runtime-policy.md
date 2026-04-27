# Universal Runtime Policy

MCPace should behave like one local runtime for many AI/MCP clients, not like a
bag of copied client configs. The product goal is one stable entry point:

```text
http://127.0.0.1:39022/mcp
```

Every local client should point at that endpoint or at the MCPace stdio launcher.
After that, MCPace owns upstream routing, process ownership, session stickiness,
project binding, and conflict control.

## Client target registry

Built-in client targets remain a safe fallback, but the runtime no longer depends
on recompiling the binary to know about a client surface. Client targets can be
loaded from:

- `clientCatalog.targets` in `mcpace.config.json`;
- `client.catalog.targets` for config-local aliases;
- `clientCatalog.paths` or `client.catalogPaths` for external catalog files;
- `MCPACE_CLIENT_CATALOG`, split with the host platform path separator.

External targets replace built-ins by normalized `id`. This is intentional: it
lets teams override paths, config shapes, or constraints when a vendor changes a
client before MCPace ships a new release. `mcpace client list --json` exposes the
source and replacement warnings so this never becomes silent drift.

## Server routing policy

Each MCP server is treated as a resource with a policy, not just as a command to
spawn. The policy vocabulary is:

- `scopeClass`: `shared-global`, `project-local`, `credential-scoped`, or
  `shared-exclusive`;
- `concurrencyPolicy`: `multi-reader`, `isolated-per-project`, `single-writer`,
  or `single-session`;
- `parallelismLimit`: `0` for unbounded safe parallel reads, or `1..n` for a
  bounded scheduler lane;
- `conflictDomain`: stable name for resources that would conflict if two chats
  touched them at once;
- `projectRootMode`: `required` or `optional`;
- `worktreeBinding`: `project-root`, `workspace-roots`, or `none`;
- `browserProfileMode`: `project-session`, `project-shared`, `host`, or `none`;
- `hostLock`: `desktop-session`, `browser-profile`, `capture-session`,
  `host-service`, or `none`;
- `startupStrategy`: `lazy-shared`, `lazy-per-project`, `lazy-per-profile`,
  `lazy-per-credential`, `singleton-host`, or another explicit scheduler hint;
- `routingGroup`: human-readable lane such as `shared`, `workspace`, `project`,
  `credential`, `browser`, or `desktop`.

`mcpace client plan --json` now renders the derived scheduler fields per server:
`processPartition`, `projectBindingKey`, `worktreeBindingKey`, `conflictDomain`,
`hostLockKey`, `browserProfileKey`, `parallelismLimit`, `schedulerLane`, and
`startupStrategy`.

## Correct parallelism model

The scheduler should parallelize only when the server policy says the underlying
resource is safe to share.

| Server class | Scheduler behavior |
| --- | --- |
| Ephemeral read-only or service-managed servers | parallel pool, optionally bounded by `parallelismLimit` |
| Project-local servers such as git, Lean context, Serena, PDF, SQLite | one process partition per resolved project root; serialize per project instance |
| Credential-scoped servers such as GitHub, Sentry, Postgres, Brave Search | partition by credential profile; allow bounded parallel reads when safe |
| Browser automation | one browser profile key per project/session unless the policy declares a shared host profile |
| Desktop/Windows automation | singleton host lock by conflict domain; never let two client chats drive the same desktop session concurrently |
| Capture/host services such as screen/capture tools | singleton or host-service queue unless policy proves read-only fan-out |

The important distinction is between parallelizing work and parallelizing access
to a mutable host resource. Browser and desktop servers may support multiple MCP
requests, but the underlying page, profile, UI, or OS session is not necessarily
safe to drive from two unrelated client chats.

## Browser-style servers

Browser automation needs a profile key, not just a process key. The safe default
is:

```text
browser-profile:<conflict-domain>|project:<project-root>|session:<lease>
```

That lets two projects run separately and prevents two chats in the same project
from corrupting one another unless the policy explicitly chooses a shared
project or host profile. MCPace should then serialize mutations through the
`browser-profile-queue` lane.

## Desktop and Windows-style servers

Desktop automation servers need a host lock. For Windows desktop MCP, the safe
default is:

```text
host-lock:windows-desktop|kind:desktop-session
```

That lock is shared across clients and chats. A session lease alone is not enough
because two clients can have different session ids while still touching the same
visible desktop.

## Project-local servers

Project-local servers should not be shared globally. The planner derives:

```text
project:<project-root>
worktree:project-root|project:<project-root>
```

When no project root is available, the route becomes pending and emits a warning.
The runtime should refuse shared routing or create a conservative isolated
placeholder until the client provides roots, cwd, or explicit `--project-root`.

## Zero-touch setup

`mcpace client install all` is the operational shortcut for local workstations.
It walks the loaded client catalog, filters local targets with install support,
and writes only MCPace-owned blocks/entries. Unsupported or cloud-only targets
are skipped, not guessed.


## Runtime lease enforcement now present

`mcpace hub lease` is the first executable enforcement layer over the planner.
It is deliberately file-backed so every local ingress can share the same lock
state before the full process pool exists.

Commands:

```bash
mcpace hub lease list --json
mcpace hub lease acquire --json --server <name> \
  --client-id <client> --session-id <session> --project-root <path>
mcpace hub lease renew --json --lease-id <lease-id> --ttl-ms 120000
mcpace hub lease release --json --lease-id <lease-id>
```

Acquisition derives the route from `client plan`, refuses pending/projectless
routes, prunes expired leases, and then enforces one of two gates:

- `requestMutexKey` for exclusive host locks, browser-profile queues,
  single-session servers, single-writer servers, and project-local serializers;
- `capacityKey` + `parallelismLimit` for parallel-safe pools.

The lease store lives at `data/runtime/hub/leases.json`; a short-lived
`leases.lock` prevents concurrent writers, and stale lock files are recovered
when their recorded `createdAtMs` is older than the lock TTL. The current layer
is still an admission controller: live MCP upstream forwarding, process pools,
request cancellation, and stale-result suppression remain the next runtime
layer.

The MCP compatibility server exposes the same operator surface as tools:
`runtime_leases`, `runtime_acquire`, `runtime_renew`, and `runtime_release`.
That lets stdio clients participate in the same scheduler contract instead of
inventing per-client locking.
