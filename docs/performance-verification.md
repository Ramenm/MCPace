# Performance verification

This project uses a two-layer performance proof model:

1. **Source-level smoke regression**: cheap, deterministic checks that run in a
   constrained packaging sandbox without a Rust host binary.
2. **Host-specific runtime proof**: real `mcpace` release binaries on Ubuntu,
   macOS, Windows, and target architectures after Rust build/test/clippy pass.

The source-level pass exists to stop obvious regressions in boundedness,
telemetry, and benchmark wiring. It does not claim production latency numbers.

## Run the smoke pass

```bash
npm run verify:performance
```

The command writes:

- `reports/performance-smoke-latest.json`
- `reports/performance-smoke-latest.md`

It runs four checks:

- local runtime HTTP benchmark wiring against an in-process mock endpoint using
  `scripts/benchmark-runtime.mjs`;
- synthetic tool-scale pressure with bounded top-k search, projection, paging,
  and heap budget checks;
- mixed-upstream topology pressure with callable, blocked, failed, disabled, and
  unsupported upstream classes;
- upstream fail-safe pressure with stale cache, circuit breaker, retry, and
  per-server isolation semantics.

## Latency gates

By default, the smoke pass records p50/p95/p99 latency but does not enforce a
fixed p95 budget. A hard number from one laptop or sandbox is not portable
release proof. After real baselines exist for each supported host family, run:

```bash
node scripts/performance-smoke.mjs --max-http-p95-ms <accepted-baseline-plus-margin>
```

Use host-specific thresholds only after they have been reviewed and recorded.


## Overhead decomposition

```bash
npm run verify:mcp-overhead-decomposition
```

This gate separates hub overhead into measurable buckets: JSON-RPC
parse/stringify, route lookup, visibility projection, scheduler locking, and
metadata signal classification. It uses synthetic inventories and previously
recorded package metadata only; it does not start random MCP servers and does not
call MCP tools.

The runtime HTTP benchmark now uses keep-alive by default so it measures steady
request overhead rather than forcing a new TCP connection for every request. To
measure connection churn explicitly, run:

```bash
node scripts/benchmark-runtime.mjs --no-keep-alive --json
```

The optimization invariants are:

- route calls use a prebuilt qualified-name index instead of scanning every
  visible tool;
- repeated same client/session/project discovery uses a visibility projection
  cache that is invalidated on server/tool/config changes;
- scheduler lock checks stay proportional to the number of locks on the current
  operation, not to total installed servers;
- JSON-RPC payload cost is reported separately from process spawn and HTTP
  connection setup.

## What still needs real proof

The final performance release gate still needs Rust host evidence:

```bash
cargo build --release --locked
./target/release/mcpace serve
npm run benchmark:runtime -- --url http://127.0.0.1:39022 --paths /healthz,/api/resources,/mcp --requests 500 --concurrency 32
npm run verify:performance
```

Measure at least:

- cold start time;
- steady-state `/healthz` and `/api/resources` p50/p95/p99 latency;
- `/mcp` initialize and tools/list p50/p95/p99 latency;
- heap/RSS growth under repeated discovery and repeated same-context calls;
- behavior with one slow upstream mixed into many fast upstreams;
- failure isolation when upstreams time out, exit, return invalid JSON, or use
  unsupported transports.

## Pass/fail policy

A source archive may include the smoke report as evidence that the performance
harness is present and bounded. It must not use that report to claim final
runtime performance. Release-ready performance needs host-specific binary traces
and accepted thresholds.
