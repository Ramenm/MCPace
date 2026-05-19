# MCP overhead and optimization gates

MCPace should be safe first, but safety must not hide avoidable latency. This gate measures local hub overhead without starting third-party MCP servers, without executing package install scripts, and without sending `tools/call` to random servers.

## What is measured

`npm run verify:mcp-overhead-profile` records synthetic hot-path costs, and `npm run verify:mcp-fixture-overhead` records actual local MCP stdio lifecycle overhead.

1. **Metadata signal classification** for many MCP-looking package descriptors.
2. **Tool exposure indexing** for many servers and tools.
3. **Scheduling decisions** for session, project, repo, database, remote-session, credential, provider-budget, and browser locks.
4. **Fixture stdio lifecycle**: cold process spawn + `initialize` + `notifications/initialized` + `tools/list`, then warm repeated `tools/list` on one initialized session.

The profile is synthetic on purpose. Random package code is not a benchmark input because it would mix hub overhead with arbitrary third-party startup, credential, network, and tool side effects.

## Why this matters

The hub should avoid four common regressions:

- classification drift between mass package surveys and adaptive runtime profiling;
- tool-list fan-out/search that grows linearly in every request instead of using exact maps plus bounded top-K projection;
- lock/scheduler logic that accidentally becomes expensive under many clients/chats;
- npm launcher overhead being confused with native binary runtime overhead.

The shared signal module is `scripts/lib/mcp-signal-policy.mjs`. Mass registry survey, adaptive evidence profiling, and overhead profiling use it so signal policy does not fork into multiple hidden taxonomies.

## Commands

```bash
npm run verify:performance
npm run verify:overhead:quick
npm run verify:orchestration
npm run verify:overhead:deep
npm run verify:overhead:full
npm run verify:overhead-audit
npm run verify:mcp-overhead-benchmark
npm run verify:mcp-overhead-stress
npm run verify:mcp-fixture-overhead
npm run verify:mcp-overhead-profile
npm run benchmark:mcp-overhead-profile
npm run benchmark:mcp-overhead
npm run benchmark:mcp-overhead-stress
npm run benchmark:mcp-fixture-overhead
```

`verify:performance` is intentionally small: it keeps the existing HTTP and simulation smoke coverage only. `verify:overhead:quick` is the default source gate for overhead regressions: launcher audit, compact benchmark, 100-server/100,000-tool stress, and fixture cold/warm MCP lifecycle. `verify:overhead:deep` runs the heavier pressure, decomposition, profile, and deep audit lanes. `verify:overhead:full` is quick plus deep for an explicit all-overhead pass. `verify:orchestration` uses the quick lane so normal local checks do not repeatedly pay every heavyweight benchmark; `verify:hardening` runs the deep lane after `verify:experience`, which already covered the quick lane.

## Optimization policy

- Keep random MCP package surveys metadata-only unless a package is explicitly selected for a safe live probe.
- Keep server enablement disabled by default after survey/install planning.
- Cache tool indexes by current `tools/list` evidence version rather than rebuilding per call; large catalog search should use bounded candidate projection, not full scans.
- Cache host platform/libc detection inside the npm launcher process; repeated binary resolution should stay sub-millisecond p95.
- Keep remote Streamable HTTP session affinity conservative until live evidence proves stateless behavior.
- Use the native binary directly in tight automation loops when npm launcher startup matters.
- Do not add new runtime dependencies for measurement unless the project-local tooling path requires them.
- Keep quick/local verification, deep verification, and full ad-hoc verification separate, so the project catches overhead regressions without making every orchestration or hardening check repeat heavyweight benchmarks.

## Interpreting results

A passing report means the local source snapshot has bounded overhead for the configured scenario. It does not prove final native release performance unless the Rust binary has been rebuilt and host-specific Rust checks pass.

## Latest source-snapshot overhead findings

The current source snapshot records these local measurements in `reports/`:

- `overhead-audit-latest`: native binary `--version` is much faster than the npm/Node wrapper; the wrapper is acceptable for human CLI use but should not be used as a per-tool hot path.
- `mcp-overhead-benchmark-latest`: 100 packages, 100 servers, 5,000 tools, and 20,000 scheduling decisions pass bounded classification, registry, scheduling, heap, disabled-server, unknown-tool, and review-gate checks.
- `mcp-overhead-decomposition-latest`: indexed route lookup is checked against linear scan, visibility projection cache hit/miss behavior is measured, and JSON-RPC serialization overhead is separated from scheduler overhead.
- `mcp-overhead-stress-latest`: 100 synthetic servers, 100,000 tools, 25,000 scheduler operations, and 1,000 metadata profiles pass bounded top-k, projection, known-tool lookup, disabled/review gate, and heap checks.
- `mcp-race-condition-audit-latest`: the scheduler fuzz lane now runs multiple deterministic seeds and verifies lock ownership, max-in-flight, disabled-server, unknown-tool, review-gate, session, credential, and browser-context invariants.

## Deep 100-server overhead audit

`npm run verify:mcp-overhead-deep` adds a broader decomposition pass for the automatic/evidence-first hub path. It uses the shared `scripts/lib/mcp-evidence-profile.mjs` profile inference library instead of duplicating a separate classifier, then measures:

- `mcp_settings.d`-style config fragment parse/merge at 100-server scale;
- cold and cached evidence-profile refresh overhead;
- worker-plan materialization from stateful/stateless lock domains;
- tool route index, bounded search candidate retention, and exact route lookup overhead;
- scheduler lock routing across sessions, projects, repositories, database paths, transport sessions, credentials, tenants, providers, browser contexts, and host sessions;
- presence of the live 100-package metadata survey safety proof.

The audit is intentionally synthetic and metadata-only. It must not install packages, start random MCP binaries, send `initialize` to random servers, or call `tools/call`. Live package execution belongs to explicit safe probes, not overhead measurement.

Use the larger local benchmark only when the host can absorb it:

```bash
npm run benchmark:mcp-overhead-deep
```

A useful optimization target is not “run every server faster”; it is “avoid doing work until it is needed.” The fastest safe path is: merge config fragments once, cache profile decisions by normalized source fingerprint, build route indexes from current `tools/list` evidence, keep disabled/review-gated servers cold, and route work through the smallest lock scope that preserves client/chat/session isolation.


## Anti-duplication and optimization changes

The overhead gates now share two small libraries instead of each script carrying its own local mini-policy:

- `scripts/lib/mcp-signal-policy.mjs` is the shared metadata signal/policy classifier used by mass package survey, overhead benchmark, overhead stress, and evidence/profile code paths.
- `scripts/lib/bounded-top-k.mjs` is the shared bounded top-k helper used by large tool-scale simulations, so search/projection stress does not sort and retain unbounded candidate lists.

The stress scheduler uses wake-one lock queues instead of waking every waiter on each released lock. That avoids a thundering-herd pattern where thousands of queued operations repeatedly re-check the same still-locked domains. The measurement stays conservative: it still blocks unknown tools, disabled servers, browser/shell review gates, project/repo/db/session/credential/transport locks, and default enablement remains false for random packages.
