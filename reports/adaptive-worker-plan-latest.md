# Adaptive worker plan

Generated: 2026-05-19T10:53:07.480Z

Status: **pass**

Plans: 13; blockers: 0; warnings: 10.

| Server | Source | Safety | Pool | Workers | In-flight/worker | Affinity | Locks | Consent | Budget |
|---|---|---|---|---:|---:|---|---|---:|---|
| unknown-stdio-npx | edge-fixture | P0_unknown_stdio | process-pool | 1 | 1 | clientInstanceId, sessionId, projectRoot | server:exclusive | yes | unknown |
| legacy-sse | edge-fixture | PX_legacy_compat | legacy-disabled | 0 | 0 | none | legacy-transport:exclusive | yes | free |
| remote-streamable-http | edge-fixture | P2_session_safe | remote-http-session-pool | 1 | 1 | transportSessionId, sessionId, credentialProfile, tenantId | transport-session:exclusive, credential:remote-origin-or-credential:exclusive | no | metered |
| stateless-remote-http | edge-fixture | P4_stateless_remote_candidate | remote-http-shared-pool | 8 | 1 | credentialProfile, tenantId, providerBudgetKey | provider-budget:token-bucket | no | metered |
| credential-scoped-api | edge-fixture | P2_session_safe | credential-session-pool | 1 | 1 | credentialProfile, sessionId, tenantId | credential:credential-profile:exclusive, tenant:exclusive | no | metered |
| project-filesystem-write | edge-fixture | P3_project_safe | project-pool | 1 | 1 | projectRoot, sessionId, clientInstanceId | file:write, project:write | no | free |
| repo-git-write | edge-fixture | P3_project_safe | project-pool | 1 | 1 | projectRoot, sessionId, clientInstanceId | repo:write, project:write | no | free |
| browser-automation | edge-fixture | PX_forbidden_browser_until_context_isolated | session-pool | 1 | 1 | browserContextId, sessionId, clientInstanceId, transportSessionId | browser-context:exclusive, host-session:exclusive | yes | free |
| shared-exclusive-desktop | edge-fixture | PX_forbidden_browser_until_context_isolated | session-pool | 1 | 1 | browserContextId, sessionId, clientInstanceId, transportSessionId | browser-context:exclusive, host-session:exclusive | yes | free |
| readonly-stdio-candidate | edge-fixture | P1_readonly_candidate | process-pool | 4 | 1 | clientInstanceId, sessionId, projectRoot | server:exclusive | no | free |
| stateful-memory | edge-fixture | P2_session_safe | singleton | 1 | 1 | tenantId | context-store:exclusive, session:exclusive | no | free |
| local-database | edge-fixture | P3_project_safe | project-pool | 1 | 1 | projectRoot, sessionId, clientInstanceId | db:exclusive, project:write | no | free |
| oci-unknown | edge-fixture | P0_unknown_stdio | process-pool | 1 | 1 | clientInstanceId, sessionId, projectRoot | server:exclusive | yes | unknown |

## Invariants

- PASS profiles-materialized: Runtime server profiles resolve to worker plans when upstreams are configured; an empty source snapshot is valid.
- PASS edge-cases-materialized: Synthetic adaptive edge cases resolve to worker plans.
- PASS unknown-safe: Unknown servers remain one in-flight per worker.
- PASS legacy-disabled: Legacy transports are disabled for worker scheduling.
- PASS affinity-boundaries: Every worker plan has required affinity, lock, consent, and degradation boundaries.

## Warnings

- unknown-stdio-npx: generated conservative plan; safe probes required before raising concurrency.
- unknown-stdio-npx: consent/review gate remains required before risky execution.
- legacy-sse: consent/review gate remains required before risky execution.
- remote-streamable-http: budget/rate-limit guardrail must be enforced at runtime.
- stateless-remote-http: budget/rate-limit guardrail must be enforced at runtime.
- credential-scoped-api: budget/rate-limit guardrail must be enforced at runtime.
- browser-automation: consent/review gate remains required before risky execution.
- shared-exclusive-desktop: consent/review gate remains required before risky execution.
- oci-unknown: generated conservative plan; safe probes required before raising concurrency.
- oci-unknown: consent/review gate remains required before risky execution.
