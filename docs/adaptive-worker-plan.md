# Adaptive worker plans

MCPace adaptive orchestration has two layers:

1. **Server profile**: what MCPace believes about a server from static metadata, safe probes, and runtime evidence.
2. **Worker plan**: the concrete scheduler decision derived from that profile: pool model, pool key, worker count, in-flight limit, affinity keys, locks, consent gate, budget class, and degradation behavior.

The worker plan is intentionally more operational than the profile. It answers: if this server is called from a chat/session/client, which pool handles it, what state boundary keeps it isolated, and what must happen when the profile is wrong.

## Required invariants

- Unknown stdio servers can scale only by isolated workers and keep `maxInFlightPerWorker=1` until safe probes pass.
- Legacy SSE remains `legacy-disabled` and must not be silently folded into Streamable HTTP.
- Remote Streamable HTTP uses transport/session identity and credential/provider budgets as scheduling boundaries.
- Browser automation requires session and browser-context affinity.
- Project/file/repo tools carry project/resource locks.
- Credential-scoped tools carry credential affinity and auth-mixup degradation.
- Any high-risk or unknown profile requires consent/review before risky execution.
- Runtime degradation can lower concurrency or disable a server; it must not raise trust on errors.

## Command

```bash
npm run verify:adaptive-worker-plan
```

The command writes:

- `reports/adaptive-worker-plan-latest.json`
- `reports/adaptive-worker-plan-latest.md`

It is source-level evidence. It does not execute untrusted MCP servers, paid APIs, package installers, or destructive tools.

## Why this exists

The previous adaptive pass produced classifications. This pass materializes those classifications into scheduler decisions. That makes it possible to test the operational boundaries before Rust runtime implementation catches up fully: each profile now becomes a plan with explicit affinity, locks, consent, budget, and degradation rules.
