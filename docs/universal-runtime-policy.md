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
- `stateProfileMode`: `project-session`, `project-shared`, `host`, or `none`;
- `hostLock`: `desktop-session`, `state-profile`, `capture-session`,
  `host-service`, or `none`;
- `startupStrategy`: `lazy-shared`, `lazy-per-project`, `lazy-per-profile`,
  `lazy-per-credential`, `singleton-host`, or another explicit scheduler hint;
- `routingGroup`: human-readable lane such as `shared`, `workspace`, `project`,
  `credential`, `demo-server`, or `desktop`.

`mcpace client plan --json` now renders the derived scheduler fields per server:
`processPartition`, `projectBindingKey`, `worktreeBindingKey`, `conflictDomain`,
`hostLockKey`, `stateProfileKey`, `parallelismLimit`, `schedulerLane`, and
`startupStrategy`.

### What MCPace can infer automatically

MCPace uses automatic signals where the MCP protocol or the client gives them:

- MCP `roots` / explicit `projectRoot` / cwd metadata can bind project and
  workspace-scoped servers.
- `Mcp-Session-Id`, `X-MCPace-Session-Id`, `X-Codex-Session-Id`, or explicit
  `sessionId` can split chat/session-affine upstream pools.
- tool `annotations` such as `readOnlyHint`, `destructiveHint`,
  `idempotentHint`, and `openWorldHint` can inform risk summaries when a
  trusted upstream server actually sends them.
- package/registry metadata can help discover installation and command shapes.

Those signals are not a complete concurrency contract. The MCP spec defines
ToolAnnotations as hints, not as trusted proof, and it does not currently define
a standard field for "parallel-safe", "single writer", "per state profile",
"per desktop session", or "this memory store is scoped to one chat". Therefore
MCPace's safe default is:

1. trust protocol hints only as advisory metadata;
2. prefer explicit `mcpace.config.json` server policies for routing;
3. use `toolPolicies` for sensitive tool-level mutation/control gates;
4. serialize or isolate unknown mutable resources instead of guessing from
   descriptions alone.

`upstream_policy_audit` operationalizes that rule for any configured MCP server.
It reads live `tools/list` output, reports annotation keys and generic advisory
risk classes, shows matching declarative `toolPolicies`, and flags
unprotected guard-recommended tools or unknown/unannotated tools for review.
The audit does not add hidden enforcement; only `toolPolicies` authorize or
block `upstream_call` / `upstream_batch`.

`upstream_policy_suggest` adds the automation boundary: it converts unprotected
guard-recommended audit findings into copyable declarative policy candidates
using stable naming rules (`interaction-control` stays shared, generic mutation
becomes `<server>-mutation`, and allow arguments become `allow<RiskClass>` in
PascalCase). Suggestions remain dry-run output until a config update applies
them, because MCP annotations and name patterns are useful signals but not a
complete trust contract.

`surface_manifest` is the transparency boundary. It reports the exact
top-level MCPace tools returned by `tools/list`, states that configured upstream
tools remain upstream rather than being disguised as native MCPace tools, and
can include a live `upstream_catalog` snapshot when a caller wants the full
current upstream count. This keeps the small wrapper surface honest: speed comes
from explicit discovery, caching, batching, and pooling, not from hiding what is
really being proxied.

## Correct parallelism model

The scheduler should parallelize only when the server policy says the underlying
resource is safe to share.

| Server class | Scheduler behavior |
| --- | --- |
| Ephemeral read-only or service-managed servers | parallel pool, optionally bounded by `parallelismLimit` |
| Project-local servers such as git, Lean context, Serena, PDF, SQLite | one process partition per resolved project root; serialize per project instance |
| Credential-scoped servers such as GitHub, Sentry, Postgres, Brave Search | partition by credential profile; allow bounded parallel reads when safe |
| Stateful interaction automation | one state profile key per project/session unless the policy declares a shared host profile |
| Desktop/Windows automation | singleton host lock by conflict domain; never let two client chats drive the same desktop session concurrently |
| Capture/host services such as screen/capture tools | singleton or host-service queue unless policy proves read-only fan-out |
| Sequential-thinking / scratchpad-style reasoning tools | session-affine serialization; do not merge two chats into one thought chain |
| Persistent memory graphs | single writer over the backing store, with explicit mutation gates when writes affect cross-chat memory |
| Filesystem roots | single writer over workspace roots unless trusted read-only annotations and tool-level read/write routing prove a narrower safe lane |
| Git and SQLite project tools | project-local serialization, with mutation tools guarded by `toolPolicies` and read/status/schema tools left available |
| Lean/Serena project context tools | project-local serialization, with shell/edit/source-memory mutations guarded by declarative `toolPolicies` |
| external MCP server host bridge | state-profile serialization; navigation/action/JavaScript/dialog/download/file/permission controls require explicit interaction-control opt-in |
| Playwright-style interaction canaries | isolated project/session stateful lane; state-changing control tools require explicit risk opt-in even when upstream annotations are present |

The important distinction is between parallelizing work and parallelizing access
to a mutable host resource. Stateful and desktop servers may support multiple MCP
requests, but the underlying page, profile, UI, or OS session is not necessarily
safe to drive from two unrelated client chats.

## Stateful interaction servers

Stateful interaction automation needs a profile key, not just a process key. The safe default
is:

```text
state-profile:<conflict-domain>|project:<project-root>|session:<lease>
```

That lets two projects run separately and prevents two chats in the same project
from corrupting one another unless the policy explicitly chooses a shared
project or host profile. MCPace should then serialize mutations through the
`state-profile-queue` lane.

## Desktop and Windows-style servers

Desktop automation servers need a host lock. For Windows desktop MCP, the safe
default is:

```text
host-lock:windows-desktop|kind:desktop-session
```

That lock is shared across clients and chats. A session lease alone is not enough
because two clients can have different session ids while still touching the same
visible desktop.

`windows-mcp` must be enabled through an explicit desktop-control profile rather
than hidden inside the safe default profile. The current MCPace-compatible
transport is the stdio bridge (`uvx windows-mcp`), not an assumed localhost HTTP
endpoint. Generic HTTP upstream fan-out remains a separate runtime capability;
desktop-control servers should not be auto-started just because their package is
installed.

The desktop host lock is necessary but not sufficient for safe use. MCPace also
supports declarative tool policies in `mcpace.config.json`:

```json
{
  "toolPolicies": [
    {
      "riskClass": "desktop-observation",
      "allowArgument": "allowDesktopObservation",
      "tools": ["Snapshot", "Screenshot", "Scrape"]
    }
  ]
}
```

The bridge enforces these policies generically for any configured upstream
server. A blocked call can be authorized by the policy's convenience boolean
(`allowDesktopObservation=true`), by the generic allow-argument list
(`allowArguments=["allowDesktopObservation"]`), or by the generic risk-class
list (`allowToolRiskClasses=["desktop-observation"]`). These are explicit
`upstream_call` / `upstream_batch` arguments; they are not inferred from chat
intent, session id, or profile activation. A profile plus platform support makes
the server reachable; the per-call policy flag makes a sensitive action
authorized for that one bridge request.

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

- `requestMutexKey` for exclusive host locks, state-profile queues,
  single-session servers, single-writer servers, and project-local serializers;
- `capacityKey` + `parallelismLimit` for parallel-safe pools.

The lease store lives at `data/runtime/hub/leases.json`; a short-lived
`leases.lock` prevents concurrent writers, and stale lock files are recovered
when their recorded `createdAtMs` is older than the lock TTL. `hub lease list
--json` exposes both active lease records and derived active session records, and
`hub status` summarizes active lease/session counts for operator visibility. The
current layer is still an admission controller: durable process pools,
transport-level request cancellation, and cross-request stale-result suppression
remain the next runtime layer.

The MCP compatibility server exposes the same operator surface as tools:
`runtime_leases`, `runtime_acquire`, `runtime_renew`, and `runtime_release`.
That lets stdio clients participate in the same scheduler contract instead of
inventing per-client locking.
