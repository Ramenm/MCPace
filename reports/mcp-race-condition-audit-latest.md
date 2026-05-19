# MCP race-condition audit

Generated: 2026-05-19T12:36:12.682Z
Status: **pass**

Operations: 15000; seeds: 5; started: 6369; disabled blocks: 5987; unknown-tool blocks: 957; review-gate blocks: 1687; conflicts delayed: 735688.
Overhead: 111.01 ms (135.122 ops/ms).

## Checks

- PASS simulation-drains-without-races: Scheduler fuzz simulation drains without overlapping lock/max-in-flight races.
- PASS all-profile-kinds-covered: covered 9/9 profiles
- PASS disabled-blocks: Disabled servers block before scheduling.
- PASS unknown-tool-blocks: Unknown tools block before forwarding.
- PASS review-gate-blocks: Unknown/high-risk profiles block behind review gate.
- PASS safe-work-starts: Safe enabled operations still run.
- PASS race-audit-overhead-bounded: 15000 operations across 5 seeds in 111.01ms
- PASS known-tool-gate: Brokered calls validate requested tool against current tools/list.
- PASS unknown-tool-explicit-override: Unknown upstream tools require explicit override.
- PASS safe-probe-no-tool-call: Live probe initializes and lists tools only; it never calls tools.
- PASS server-side-requests-not-fulfilled: Probe rejects unexpected server-side requests.
- PASS remote-session-affinity: Remote HTTP session pool carries transportSessionId affinity.
- PASS credential-affinity: Credentialed pools carry credentialProfile affinity.
- PASS no-default-upstreams: Source snapshot has no enabled upstream MCP servers by default.
