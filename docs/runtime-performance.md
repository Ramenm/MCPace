# Runtime performance and resource controls

MCPace is intentionally dependency-light in the local bootstrap path, so this
pass improves throughput with bounded native threads and explicit resource
limits instead of adding a new async runtime.

## What changed in this pass

- Local HTTP serving (`mcpace serve` and `mcpace dashboard`) now accepts multiple
  requests concurrently through a fixed worker pool. Accepted sockets are handed
  to workers with a zero-buffer rendezvous channel, so request bursts apply
  backpressure instead of spawning a new OS thread per request.
- `/api/overview` and the `runtime_diagnostics` MCP tool now fan out independent
  read-only JSON commands in parallel instead of waiting for each diagnostic
  command in sequence.
- `/healthz` has a tiny in-process readiness cache to absorb local health-poll
  bursts without turning every poll into a full readiness command. Health errors
  are not hidden behind stale success responses.
- `/api/overview` now has a short in-memory cache with single-process request
  coalescing. Repeated dashboard refreshes reuse the latest overview for the
  configured TTL instead of spawning the same diagnostic commands again. If a
  refresh fails after a previous snapshot exists, MCPace returns the stale
  snapshot with `cache.stale=true` and `cache.refreshError` instead of blanking
  the operator view.
- `/healthz` now has a short process-local readiness cache so aggressive local
  supervisors do not repeatedly spawn the full readiness command. Like overview,
  it accepts `?refresh=1` / `?noCache=true` for an explicit fresh probe.
- `/api/overview` and `/healthz` now include a `runtime` block with live local
  HTTP counters, configured request limits, cache TTLs, available parallelism,
  and current upstream session-pool occupancy.
- HTTP request parsing now rejects oversized request lines, headers, and bodies
  before allocating the full body. It also applies read/write timeouts on every
  accepted TCP stream.
- Upstream discovery/probe/audit/suggestion fan-out runs through a bounded
  worker queue instead of spawning one OS thread for every configured server or
  waiting for whole batches to finish before dispatching more work.
- Upstream `tools/list` cache misses are coalesced per server/config key, so
  concurrent identical misses share a single loader rather than stampeding the
  upstream process. The short cache is also capped to bound memory use.
- The in-process upstream session pool now derives its default size from
  available host parallelism instead of a fixed magic number, and HTTP upstream
  calls use context-stable pool shards to reduce mutex contention between
  unrelated client/session/project contexts.
- Release builds now use a tighter Cargo release profile so shipped binaries
  favor runtime performance and smaller artifacts over fastest compile time.

## Defaults

Defaults are centralized in `src/resources.rs`:

| Area | Default |
| --- | --- |
| HTTP request worker pool | `available_parallelism * 4`, clamped to `4..64` |
| Upstream discovery/probe worker limit | `available_parallelism * 2`, clamped to `1..16`, never above task count |
| In-process upstream session pool limit | `available_parallelism * 2`, clamped to `4..16` total |
| HTTP upstream session-pool shards | `available_parallelism`, clamped to the total pool limit and `1..8` |
| HTTP read/write timeout | `30_000` ms |
| HTTP max request body | `1_048_576` bytes |
| HTTP max request line | `8 KiB` |
| HTTP max header line | `8 KiB` |
| HTTP max header block | `64 KiB` |
| HTTP max header count | `128` |
| Dashboard overview cache TTL | `1_500` ms; set `--overview-cache-ms 0` to disable |
| Dashboard health cache TTL | `1_000` ms process-local readiness cache; use `/healthz?refresh=1` to bypass |
| Health/readiness cache TTL | `1_000` ms process-local cache for `/healthz` |
| Upstream `tools/list` cache | `30` seconds, capped at `128` entries per process |

`available_parallelism` is used because it is the standard-library estimate of
how much parallel work the process should use by default. It is still only a
default: constrained hosts, unusual CPU topologies, and real upstream behavior
can make manual tuning worthwhile.


## Environment override knobs

The CLI flags remain the most explicit tuning surface, but local services can now inherit bounded resource overrides from environment variables when no flag is supplied:

| Variable | Applies to | Behavior |
| --- | --- | --- |
| `MCPACE_HTTP_MAX_CONNECTIONS` | local HTTP worker pool | positive integer, capped at 256 |
| `MCPACE_HTTP_IO_TIMEOUT_MS` | socket read/write timeout | positive integer milliseconds |
| `MCPACE_HTTP_MAX_BODY_BYTES` | JSON-RPC request body limit | positive integer bytes |
| `MCPACE_DASHBOARD_OVERVIEW_CACHE_MS` | `/api/overview` cache | non-negative milliseconds; `0` disables |
| `MCPACE_DASHBOARD_HEALTH_CACHE_MS` | `/healthz` cache | non-negative milliseconds; `0` disables |
| `MCPACE_UPSTREAM_WORKERS` | upstream discovery/probe fan-out | positive integer, capped at 64 and never above task count |
| `MCPACE_UPSTREAM_SESSION_POOL_LIMIT` | in-process upstream session reuse | positive integer, capped at 128 |
| `MCPACE_UPSTREAM_SESSION_POOL_SHARDS` | session-pool mutex sharding | positive integer, capped at pool size and 32 |

Invalid values are ignored so an accidental bad shell export does not prevent the local endpoint from starting. Explicit command-line flags still win because they are parsed into the serve/dashboard config before defaults are used.

## Tuning commands

For a foreground one-port endpoint:

```bash
mcpace serve --port 39022 --max-connections 32 --io-timeout-ms 30000 --max-body-bytes 1048576 --overview-cache-ms 1500
```

For a background endpoint:

```bash
mcpace serve start --json --max-connections 32 --io-timeout-ms 30000 --max-body-bytes 1048576 --overview-cache-ms 1500
mcpace serve status --json
```

For the dashboard:

```bash
mcpace dashboard --max-connections 16 --io-timeout-ms 30000 --max-body-bytes 1048576 --overview-cache-ms 1500
```

For one-command setup and user-level autostart, the same resource flags are
forwarded into the background `serve start` process and service target args:

```bash
mcpace setup --json --max-connections 32 --io-timeout-ms 30000 --max-body-bytes 1048576 --overview-cache-ms 1500
mcpace service install --json --max-connections 32 --io-timeout-ms 30000 --max-body-bytes 1048576 --overview-cache-ms 1500
```

## Resource behavior

- `--max-connections` sets the fixed local HTTP worker-pool size. The listener
  uses a zero-buffer handoff to the pool, so saturated workers backpressure the
  accept loop without per-request thread creation or an unbounded request queue.
  Overview and health responses also include a `runtime.http` object with active,
  accepted, completed, failed, and max-observed active connection counters.
- `--io-timeout-ms` bounds blocking socket reads/writes. Keep it generous enough
  for local upstream calls, but low enough to prevent dead client connections
  from occupying workers indefinitely.
- `--max-body-bytes` applies to JSON-RPC POST bodies before allocation. Raise it
  only if a client has a legitimate need to send large arguments through `/mcp`.
- `--overview-cache-ms` controls the short dashboard overview cache. The default
  keeps the dashboard responsive during refresh bursts; `?refresh=1` or
  `?noCache=true` on `/api/overview` bypasses it for explicit diagnostics; `0`
  disables it completely. The dashboard's manual refresh and post-action refresh
  force a fresh overview, while automatic polling can reuse the cache and shows
  the cache state in the UI. If a refresh fails after a previous snapshot exists,
  the response remains useful and marks `cache.stale=true`.
- `/healthz` keeps its own tiny readiness cache because health probes are often
  more frequent than human dashboard refreshes. The response still includes the
  familiar top-level `ok` and `readiness` fields, but now also reports cache
  state and live `runtime.http` counters for accepted, active, completed, failed,
  and max-observed active local connections.
- The `runtime.upstreamSessionPool` object exposes the current in-process pool
  size, configured max size, and idle TTL so operators can see whether pooled
  upstream reuse is happening or whether calls are still cold-starting.

## Runtime metadata

`/healthz` and `/api/overview` include a `runtime` object so operators can see
which surface handled the request, the available parallelism used for defaults,
HTTP resource limits and counters, cache TTLs, and the in-process upstream
session-pool size/limit/shard count. This is lightweight local telemetry; it is
not yet a persistent metrics backend.

## Release profile

`Cargo.toml` now defines:

```toml
[profile.release]
lto = "thin"
codegen-units = 1
strip = "symbols"
panic = "abort"
```

This favors a practical runtime/size profile for the CLI and local server. The
trade-off is slower release compilation and less useful stripped-symbol debug
output. Developer builds keep the normal dev profile.

## Local benchmark helper

After starting a foreground or background local endpoint, run:

```bash
npm run benchmark:runtime -- --url http://127.0.0.1:39022 --paths /healthz,/api/resources --requests 100 --concurrency 16
```

The helper issues bounded concurrent GET requests and reports throughput plus
p50/p95/p99 latency per path. It is intentionally a lightweight smoke benchmark,
not a substitute for full CI performance gates. Use it to compare local tuning
changes such as `--max-connections`, `--io-timeout-ms`, and cache TTLs before
collecting host-specific baselines.

## What still needs real-host proof

This pass improves source-level behavior and local contract coverage, but it
does not replace host benchmarking. The next runtime proof should measure:

- cold `serve start` time, `/healthz` cache-hit latency, and forced-refresh
  `/healthz?refresh=1` latency on Ubuntu, macOS, and Windows;
- p50/p95 latency for `/mcp` `initialize`, `tools/list`, and `upstream_call`;
- memory growth while repeatedly calling the same-context upstream pool and while
  repeatedly discovering many unique upstream `tools/list` cache keys;
- behavior when many configured upstream servers are probed at once, including
  one slow server mixed with many fast servers to verify queue fairness;
- stale-cache fallback behavior when a cached overview/health snapshot exists
  and a later refresh command fails;
- interaction between bounded worker limits, scheduler leases, and long-running
  upstream tools.

## Remaining engineering follow-up

- Durable cross-process upstream session ownership is still not complete.
- Transport-level cancellation still needs to propagate deeper than the current
  request-time wrapper guard.
- Per-upstream policy could grow beyond default resource-derived bounds once
  real-host traces show which servers are CPU-bound, IO-bound, or single-writer
  stateful.
- The dashboard overview cache is process-local. A future multi-process relay
  should either keep it per worker with tiny TTLs or move aggregation into a
  shared diagnostics service.
