# Runtime state, cache, restart, and reinstall lifecycle

This document is the lifecycle contract for MCPace runtime data. It exists to keep
feature work, recovery logic, tests, and release claims aligned.

## Core rules

1. **Config is durable and user-owned.** It may be changed by explicit user action
   or install/restore commands, but it must not be silently deleted during restart,
   repair, or package reinstall.
2. **State is recoverable.** Runtime state can help MCPace resume or explain what
   happened, but it must tolerate stale process ids, crashed writers, and partial
   previous runs.
3. **Cache is disposable.** Cache may improve startup or tool discovery speed, but
   correctness must never depend on a cache hit.
4. **Ephemeral runtime facts are process-owned.** Protocol sessions, child process
   handles, locks, stop signals, and health snapshots may disappear on restart.
   Callers must be able to reinitialize instead of requiring MCPace to resurrect
   them.
5. **Durable writes must be crash-safe where practical.** Config and restore paths
   should write through a temporary file and atomic rename helper instead of direct
   in-place writes.
6. **Reports are evidence, not runtime truth.** A stale report must never override
   a fresh local check.
7. **Secrets must not be copied into reports or cache.** Cache keys may use hashes
   and metadata fingerprints, but not raw secret values.

## Storage classes

| Area | Class | Default location | Survives process restart | Survives MCPace reinstall | Owner | Invalidation / recovery |
|---|---|---|---:|---:|---|---|
| `mcpace.config.json` | Durable config | Project root or explicit config path | Yes | Yes if the project/state root remains | User / MCPace commands | Written with atomic helper; validate before use |
| `mcp_settings.json`, `mcp_settings.d/*.json` | Durable upstream registry config | Project root | Yes | Yes | User / MCPace server-source commands | Settings mtime/length participates in tool-list cache key; add/remove/toggle should be followed by a refresh/test when runtime correctness matters |
| External client config files | Durable external config | Client-specific paths outside MCPace | Yes | Yes, unless the client or user removes them | Client app + user | MCPace must create backups before patching and restore from backup when requested |
| `data/client-install-backups/**` | Durable rollback artifacts | State root | Yes | Yes if state root remains | MCPace | Can be pruned by explicit maintenance only; never needed for live protocol correctness |
| `data/runtime/hub/state.json` | Recoverable state | State root | Yes | Yes if state root remains | MCPace hub | Repair may normalize or archive corrupt state; stale pid values require probing before trust |
| `data/runtime/hub/leases.json` | Recoverable coordination state | State root | Yes, until TTL expires | Yes if state root remains | MCPace hub | Expired leases are purged; default TTL 120s, max TTL 1h |
| `data/runtime/hub/health.json` | Ephemeral health snapshot | State root | No strong guarantee | Yes as stale evidence only | MCPace hub | Treat as stale unless current process/probe confirms it |
| `data/runtime/hub/lock.json`, `stop.signal` | Ephemeral coordination files | State root | Maybe stale | Maybe stale | Current MCPace process | Repair/restart may remove stale files after pid/probe checks |
| `data/runtime/serve/state.json` | Recoverable serve marker | State root | Maybe stale | Maybe stale | MCPace serve mode | Use only with current process checks; stale pid must not imply running service |
| `data/runtime/tool-list-cache/*.json` | Disposable disk cache | State root | Yes until TTL | Yes if state root remains, but version/protocol changes invalidate | MCPace upstream runtime | 24h TTL; key includes config root, server name, settings fingerprint, server fingerprint, MCPace version, and MCP protocol version |
| In-memory tool-list cache | Disposable memory cache | Process memory | No | No | Current MCPace process | 30s TTL; bypass with refresh paths |
| Dashboard overview/health cache | Disposable memory cache | Process memory | No | No | Current dashboard process | Short TTL; refresh/no-cache paths bypass |
| HTTP MCP sessions | Ephemeral protocol state | Process memory | No | No | Current dashboard/MCP HTTP process | Clients must call `initialize` again after restart or unknown session errors |
| Upstream stdio/http session pool | Ephemeral process/resource state | Process memory and child processes | No | No | Current MCPace process | Idle TTL 300s; restart kills/loses child handles; recreate on next request |
| `reports/**` | Verification evidence | Repository reports dir | Yes as files | Yes as files | Verification commands | Not authoritative unless generated for the current tree and current command |

## Restart behavior

### MCPace process restart

After a process restart, MCPace may reuse durable config and recoverable state, but
must not assume that protocol sessions, child processes, in-memory cache, locks, or
health snapshots are still valid.

Expected behavior:

- HTTP MCP clients should be ready to call `initialize` again when their previous
  session id is missing or expired.
- Upstream stdio sessions are recreated on demand. Child process ids from a prior
  process must not be reused as live handles.
- Hub leases may remain on disk, but expired leases are invalid and should be
  purged before decisions that depend on them.
- Stale hub/serve state is diagnostic evidence until a current process/probe
  confirms it.
- Disk tool-list cache may be reused only while the TTL and cache key still match.

### Crash during config write

Config mutation paths must prefer `runtimepaths::write_text_atomic`. The intended
failure mode is either the old complete file or the new complete file, not a
partially written config. Temporary files may be left behind after a crash and can
be cleaned by maintenance, but they must not be treated as canonical config.

### Hub repair / stop / restart

Repair logic may remove stale lock/stop files and normalize recoverable state, but
it must not delete durable user config, client backups, or source fragments.

## Reinstall and upgrade behavior

An npm reinstall or MCPace package upgrade replaces the installed package and
binary. It must not be treated as consent to delete project config, upstream
registry fragments, client config backups, or the state root.

Expected behavior:

- Durable config survives if the project directory or `MCPACE_STATE_ROOT` remains.
- Client app config may still point to `mcpace` by command name. If an absolute
  binary path was previously installed and the package location changed, run the
  client install/check flow again and inspect the diff before applying changes.
- Disk tool-list cache is invalidated when the MCPace version or MCP protocol
  version changes.
- Reinstalling an upstream tool is not always detectable when the command name and
  registry config stay unchanged. After upgrading/reinstalling an upstream server,
  run a server test with refresh or clear `data/runtime/tool-list-cache` before
  relying on cached tool lists.
- Reports generated before reinstall/upgrade are historical evidence only.

## Cache policy

Use a cache only when all of these are true:

- A miss can recompute the value from durable config or a live upstream probe.
- Stale data is bounded by TTL and by a cache key/fingerprint.
- The caller has a refresh path for correctness-sensitive checks.
- Cached values do not contain raw secrets.

Do not cache:

- protocol session truth that must be negotiated with the client;
- live child process handles across process restart;
- authorization/secret material;
- install success claims without a fresh verification command.

## Critical invariants

- Durable config writes use atomic write helpers.
- Protocol sessions are not persisted across restart.
- Child process sessions are not persisted across restart.
- Tool-list disk cache is disposable and versioned.
- Reinstall/upgrade invalidates MCPace-version-sensitive cache but does not delete
  user config.
- Client config patches create backups before changing external files.
- A fresh failed verification beats an old passing report.

## Related lifecycle contracts

- `system-lifecycle-hardening.md` extends this storage contract across install, first start, runtime, restart, crash recovery, upgrade, reinstall, uninstall, diagnostics, and release/publish.
