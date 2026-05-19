# MCP overhead pressure audit

Generated: 2026-05-19T12:36:13.457Z
Status: **pass**
Servers: 10000; fragments: 200; scheduler ops: 50000.

## Summary

| Metric | Value |
|---|---:|
| Profile avg | 2.51 us/profile |
| Fragment scan avg | 0.6 ms/fragment |
| Scheduler avg | 0.438 us/op |
| Heap delta budget total | 8.54 MiB |
| Allowed operations | 3148 |
| Review-gated operations | 12500 |
| Lock-blocked operations | 34352 |

## Safety

- startsMcpServers: false
- callsMcpTools: false
- executesThirdPartyPackages: false
- installsPackages: false
- usesNetwork: false
- mutatesProjectConfig: false
- syntheticOnly: true

## Checks

- PASS profile-throughput-budget: 2.51us/profile <= 250us
- PASS fragment-scan-budget: 0.6ms/fragment <= 12ms
- PASS scheduler-routing-budget: 0.438us/operation <= 80us
- PASS heap-budget: 8.54MiB <= 128MiB
- PASS no-random-mcp-execution: No package binaries, MCP initialize, or tools/call are executed.
- PASS unknown-and-high-risk-review-gated: 12500 review-gated operations
- PASS scheduler-lock-invariants: 0 route-key invariant violations
- PASS single-shared-profile-library: Adaptive audit and pressure audit use scripts/lib/mcp-evidence-profile.mjs.

## Optimization plan

- Keep profile inference metadata-only on connect/open; do not spawn MCP servers during catalog rendering.
- Cache profile results by normalized command/url/policy fingerprint and invalidate on mcp_settings fragment mtime/hash changes.
- Start stdio servers lazily only when a reviewed tool is actually needed; use tools/list probe as explicit server test, not as default UI refresh.
- Shard Streamable HTTP by transport session and credential profile until explicit stateless evidence exists.
- Prefer lock-key routing over global singletons so safe read-only utilities can scale without sharing filesystem/git/db/browser state.
