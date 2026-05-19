# MCP overhead pressure audit

Generated: 2026-05-19T10:36:44.254Z
Status: **pass**
Servers: 50000; fragments: 1000; scheduler ops: 250000.

## Summary

| Metric | Value |
|---|---:|
| Profile avg | 11.25 us/profile |
| Fragment scan avg | 0.55 ms/fragment |
| Scheduler avg | 1.036 us/op |
| Heap delta budget total | 44.42 MiB |
| Allowed operations | 13664 |
| Review-gated operations | 62500 |
| Lock-blocked operations | 173836 |

## Safety

- startsMcpServers: false
- callsMcpTools: false
- executesThirdPartyPackages: false
- installsPackages: false
- usesNetwork: false
- mutatesProjectConfig: false
- syntheticOnly: true

## Checks

- PASS profile-throughput-budget: 11.25us/profile <= 250us
- PASS fragment-scan-budget: 0.55ms/fragment <= 8ms
- PASS scheduler-routing-budget: 1.036us/operation <= 80us
- PASS heap-budget: 44.42MiB <= 128MiB
- PASS no-random-mcp-execution: No package binaries, MCP initialize, or tools/call are executed.
- PASS unknown-and-high-risk-review-gated: 62500 review-gated operations
- PASS scheduler-lock-invariants: 0 route-key invariant violations
- PASS single-shared-profile-library: Adaptive audit and pressure audit use scripts/lib/mcp-evidence-profile.mjs.

## Optimization plan

- Keep profile inference metadata-only on connect/open; do not spawn MCP servers during catalog rendering.
- Cache profile results by normalized command/url/policy fingerprint and invalidate on mcp_settings fragment mtime/hash changes.
- Start stdio servers lazily only when a reviewed tool is actually needed; use tools/list probe as explicit server test, not as default UI refresh.
- Shard Streamable HTTP by transport session and credential profile until explicit stateless evidence exists.
- Prefer lock-key routing over global singletons so safe read-only utilities can scale without sharing filesystem/git/db/browser state.
