# Performance decision log — 2026-04-30 follow-up passes

This log captures the questions a larger engineering team would ask before
turning local MCPace into a more scalable runtime. Each question is paired with
the decision implemented in this pass or the follow-up that still needs proof.

## Dashboard and diagnostics

**Question:** Should `/api/overview` recompute every diagnostic section on every
web UI refresh?

**Decision:** No. Overview is a read-mostly dashboard aggregate, so it now uses a
short process-local cache. The default TTL is `1_500` ms, enough to absorb UI
refresh bursts without hiding state for long. Operators can bypass it with
`/api/overview?refresh=1` or `/api/overview?noCache=true`, and can disable it
with `--overview-cache-ms 0`.

**Question:** Should the cache be durable or shared across processes?

**Decision:** Not yet. The current local server is the hot path and the TTL is
small. A future relay/multi-worker deployment should either keep tiny per-worker
caches or move overview aggregation into a shared diagnostics service.

## HTTP resource controls

**Question:** Should health checks be cached?

**Decision:** Yes, but only narrowly. `/healthz` now uses a `1_000` ms
process-local readiness cache to absorb health-poll bursts. If a cached snapshot
exists and a later explicit refresh fails, the response is marked
`cache.stale=true` and carries the refresh error so status automation and humans
do not mistake a stale snapshot for a fresh success.

**Question:** Should local HTTP expose lightweight runtime counters?

**Decision:** Yes. Overview and health JSON now include a `runtime` object with
worker-pool sizing, active/accepted/completed/failed request counters,
max-observed active requests, cache TTLs, available parallelism, and upstream
session-pool occupancy. This gives operators tuning evidence without requiring a
separate metrics service.

## HTTP resource controls

**Question:** Can a client monopolize resources with many slow connections or
large headers?

**Decision:** The listener now uses a fixed worker pool sized by
`--max-connections` rather than spawning one OS thread per request. Accepted
sockets are handed to the pool through a zero-buffer rendezvous channel, which
keeps backpressure explicit and avoids an unbounded pending queue. The same HTTP
path still applies read/write timeouts, request-line and header-line limits,
total header bytes, header count, and body size limits. These controls are
exposed through `dashboard`, foreground `serve`, `serve start`, `setup`, and
`service install` so the same runtime target can be tuned whether it is started
manually or through autostart.

## Upstream fan-out

**Question:** Should the HTTP upstream session pool be one mutex-protected map?

**Decision:** Not for the hot HTTP path. The pool limit is still bounded and
resource-derived, but the dashboard/serve HTTP bridge now splits it into
context-stable shards keyed by server/client/session/project/transport hints. A
single client session still routes consistently, while unrelated sessions contend
less on one global lock. The resulting pool shards are reported in runtime
metadata for tuning evidence. This remains in-process only; durable cross-process
ownership is still open.

## Upstream fan-out

**Question:** Should upstream discovery/probe/audit spawn one OS thread per
configured server?

**Decision:** No. Work now runs through a bounded worker queue derived from
available host parallelism. The queue preserves result order while avoiding the
previous batch-level head-of-line blocking behavior where one slow server could
delay dispatching later servers in the next batch.

**Question:** Should concurrent identical `tools/list` cache misses all launch
separate upstream probes?

**Decision:** No. In-process `tools/list` misses now use per-cache-key
singleflight/coalescing. One caller loads; concurrent callers wait and then read
the populated cache. Explicit refresh requests still bypass the cache. The cache
is capped at 128 entries per process so many changing server/config keys do not
become unbounded memory growth.

## Defaults and tuning

**Question:** Should defaults be hard-coded magic numbers?

**Decision:** Defaults are centralized in `src/resources.rs` and derive from
`std::thread::available_parallelism()` where possible. Operators can override
HTTP worker count, IO timeout, body size, and dashboard overview cache TTL.

**Question:** Should the release profile optimize for fastest compile time or a
smaller/faster distributed binary?

**Decision:** Distributed release builds now favor runtime and artifact size with
ThinLTO, one codegen unit, stripped symbols, and abort-on-panic. Developer builds
keep the normal dev profile.

## Still-open decisions

- Whether per-upstream concurrency should be globally capped by policy once real
  traces classify servers as CPU-bound, IO-bound, or single-writer stateful.
- Whether Streamable HTTP should move from wrapper-first compatibility into a
  fully durable session lifecycle with create/reuse/close and transport-level
  cancellation.
- Whether benchmark thresholds should be enforced in CI after host-specific
  baselines are gathered for Ubuntu, macOS, and Windows.
- Whether dashboard aggregation should become an internal service when the relay
  or multi-process deployment mode exists.


## Dashboard UI follow-up

**Question:** Should the manual dashboard refresh use the same short cache as
background polling?

**Decision:** No. Automatic polling can reuse the short cache, but the manual
Refresh button and post-action refresh now call `/api/overview?refresh=1` and the
hero area shows whether the returned overview was fresh, a cache hit, or an
explicit bypass. This keeps the cache helpful for load while preserving operator
intent.


## Health checks and runtime observability

**Question:** Should every `/healthz` request run the full readiness command?

**Decision:** No. Health probes can be much more frequent than human dashboard
refreshes, so `/healthz` now has a tiny process-local readiness cache with an
explicit `?refresh=1` / `?noCache=true` bypass. The response still keeps the
existing `ok` and `readiness` contract, but adds cache metadata so probes and
operators can tell when a response was reused.

**Question:** Should a dashboard refresh failure erase the last useful snapshot?

**Decision:** No. If overview or health refresh fails after a previous snapshot
exists, MCPace returns that stale snapshot with `cache.stale=true` and the
refresh error attached. This preserves situational awareness during transient
command failures while still making the stale state explicit.

**Question:** How should operators see whether resource controls are actually
being exercised?

**Decision:** `/api/overview` and `/healthz` now include a `runtime` block with
live HTTP worker counters, configured request limits, cache TTLs, available
parallelism, and upstream session-pool occupancy. This avoids hiding the
backpressure/resource behavior behind docs-only claims.

## Measurement follow-up

**Question:** Should this pass claim speedups without a repeatable measurement
harness?

**Decision:** No. A lightweight `npm run benchmark:runtime` helper now measures
selected local HTTP paths with bounded concurrency and reports throughput plus
p50/p95/p99 latency. It is a smoke benchmark for operator tuning and regression
triage; CI performance thresholds still need real Ubuntu/macOS/Windows host
baselines before becoming release gates.
