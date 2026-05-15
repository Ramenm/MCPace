# MCPace full-system lifecycle hardening contract

This document is the end-to-end hardening contract for MCPace. It extends
`runtime-state-cache-lifecycle.md` from runtime storage rules into a full product
lifecycle: install, first start, steady runtime, restart, crash recovery, update,
reinstall, uninstall, diagnostics, and release/publish.

## Non-negotiable design boundary

MCPace has four different data classes and they must not be treated the same:

| Data class | Examples | Restart | Reinstall / upgrade | Uninstall default |
|---|---|---|---|---|
| Durable user config | `mcpace.config.json`, `mcp_settings.json`, `mcp_settings.d/*.json`, external client configs | preserve | preserve | preserve |
| Durable control-plane state | project registry, lease registry, recovery markers, service target metadata | validate / migrate | validate / migrate | preserve unless `--purge-state` |
| Disposable cache | tool-list cache, overview cache, probe cache, proof cache | reuse only by TTL/key | invalidate on version/protocol/config drift | remove |
| Ephemeral runtime facts | HTTP MCP sessions, upstream child handles, in-flight cursors, lock/stop files | drop | drop | remove |

A feature is not lifecycle-safe until it declares which row it belongs to and has
a regression check that verifies restart/reinstall/uninstall behavior.

## Critical node map

| Node | Owner module | Inputs | Outputs | Failure mode to defend |
|---|---|---|---|---|
| npm launcher | `packages/npm/cli` | platform, env override, optional platform package | native `mcpace` process | wrong binary, missing exec bit, stale vendored fallback |
| platform package manifests | `packages/npm/cli-*`, `release-targets.json` | OS/CPU/libc target | platform binary package | metadata drift, wrong target packaged |
| root discovery | `reporoot.rs`, CLI `--root` | cwd, executable path | project root | accidental state in source checkout |
| state root resolution | `runtimepaths.rs` | project root, `MCPACE_STATE_ROOT` | runtime/cache/log roots | state/config/cache mixed together |
| init | `init.rs` | root/state root | seeded JSON and runtime dirs | partial seed files after crash |
| service/autostart | `service.rs`, `setup.rs` | current executable, root, serve args | user-level launcher | stale absolute path after reinstall |
| serve lifecycle | `serve.rs` | serve args/state root | background runner, state, logs | stale PID, orphan process, broken stop/restart |
| HTTP MCP boundary | `dashboard/http_boundary.rs`, `mcp_http.rs`, `http_session.rs` | request headers/session id/body | JSON-RPC/SSE response | unsafe origin, stale session after restart |
| upstream runtime | `upstream/*` | registry config, client metadata | tool list/call result | stale tool cache, child process leaks |
| mcp source mutation | `mcp_sources/write.rs`, `import.rs` | add/remove/toggle/import args | settings fragment | partial JSON, lost server entry |
| client install/restore | `client/actions*` | client catalog, endpoint, config path | external client config | no rollback, partial config write |
| hub leases/runtime | `hub/*` | project/server/session metadata | leases, health, logs | stale lease/lock/state trusted as live |
| diagnostics | `doctor.rs`, `verify/*`, scripts | local files/tools/reports | status/support evidence | stale report treated as current truth |
| release/publish | `scripts/*publish*`, `.github/workflows/*` | fresh proofs, artifacts | npm/source release | false green, missing provenance |

## Lifecycle scenarios

### Install

Install only proves that the package and binary are present. It must not silently
enable long-lived background services or mutate external client configs. Those
remain explicit commands: `setup`, `service install`, and `client install`.

Required checks:

- launcher resolves the native binary deterministically;
- Unix binaries preserve executable bits;
- platform package metadata uses explicit OS/CPU/libc filters;
- install-readiness includes source audit, lifecycle audit, binary/package proof,
  and stale-report detection.

### First start

First start may create runtime directories and seed missing state JSON, but it
must write through the atomic helper. It must not overwrite existing user config.

Required checks:

- `init` uses `runtimepaths::write_text_atomic` for seeded JSON;
- recoverable state is validated before use;
- missing runtime dirs are created idempotently.

### Runtime

Runtime correctness must be based on durable config plus live probes, not cache
hits. Local HTTP must keep a narrow boundary: localhost by default, request size
limits, session validation, and explicit Origin/Accept handling.

Required checks:

- HTTP sessions are process-local and never persisted;
- upstream child/session pool is process-local and reset on restart;
- tool cache key includes settings fingerprint, MCPace version, and MCP protocol
  version;
- no secret values are placed in cache keys, reports, or diagnostic bundles.

### Restart

Restart is a hard boundary for transport sessions and child processes. Durable
config survives. Recoverable state may be read only after freshness/probe checks.
Old HTTP MCP session ids must be treated as unknown and the client must
re-initialize.

Required checks:

- lock/health/serve PID files are diagnostic only until probed;
- leases expire and are purged before ownership decisions;
- caches either miss or are validated by TTL/key.

### Crash recovery

Crash recovery must never require manual JSON editing as the first response. The
safe path is: inspect, archive corrupt files, reseed known baseline, report what
changed.

Required checks:

- config/source/client mutations use atomic writes;
- `repair` never deletes durable user config or client backups;
- corrupt runtime files are archived before replacement.

### Upgrade and reinstall

Upgrade/reinstall replaces code and binaries. It is not consent to delete config
or state. Any absolute autostart/client path must be reconciled against the
current executable.

Required checks:

- caches include binary/version/protocol-sensitive keys;
- service status/print expose target executable and args;
- user can rerun setup/client install with dry-run/diff before patching;
- reports from the previous version are evidence only, not current truth.

### Uninstall

Package-manager uninstall cannot be the only cleanup mechanism. MCPace needs an
explicit user-facing uninstall/cleanup orchestration path:

- preserve config by default;
- stop background processes and disable autostart;
- remove disposable cache and ephemeral runtime files;
- purge durable control-plane state only with explicit `--purge-state`;
- purge config/backups only with explicit destructive confirmation.

The native `mcpace cleanup` command handles the safe half of this contract today: cache, logs, and ephemeral runtime markers. A future destructive uninstall orchestration can build on the same policy without relying on package-manager hooks.

### Diagnostics and support bundles

Diagnostics must be layered:

1. machine-readable status snapshots;
2. operational logs with retention;
3. redacted support bundle.

A support bundle must redact token-like keys, authorization headers, cookies,
private upstream headers, and raw secret env values. It should preserve enough
correlation information to connect request id, session id, lease id, server name,
and upstream process id without leaking credentials.

### Release and publish

Release claims must be based on fresh proofs for the current tree. Old reports
are useful historical evidence, but a fresh failing check always wins over an old
passing report.

Required gates:

- source audit;
- lifecycle audit;
- npm pack/platform package verification;
- vendored binary verification only as fallback/offline proof;
- Rust compile/fmt/clippy/test on a Rust host;
- runtime trace;
- real-client trace before strengthening runtime beta claims.

## Regression checklist

- [ ] Every mutating durable write path uses `runtimepaths::write_text_atomic` or
      a documented append-only log pattern.
- [ ] Every cache has TTL, key, owner, and invalidation rule.
- [ ] Every session/process handle is explicitly process-local.
- [ ] Restart invalidates HTTP sessions and upstream child handles.
- [ ] Reinstall preserves config but forces binary/service/client path reconcile.
- [ ] Uninstall semantics are explicit and do not depend on npm uninstall hooks.
- [ ] Fresh local checks override stale reports.
- [ ] Source audit owns unsafe/FFI and large-module boundaries.
- [ ] Diagnostics are redacted and split from ordinary runtime logs.
- [ ] Release artifacts include platform metadata and provenance checks.
