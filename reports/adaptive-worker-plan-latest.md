# Adaptive worker plan

Generated: 2026-05-16T15:05:07.979Z

Status: **pass**

Plans: 14; blockers: 0; warnings: 12.

| Server | Source | Safety | Pool | Workers | In-flight/worker | Affinity | Locks | Consent | Budget |
|---|---|---|---|---:|---:|---|---|---:|---|
| filesystem | runtime-profile | P3_project_safe | project-pool | 4 | 1 | projectRoot, sessionId, clientInstanceId | project:write, file:write | no | free |
| context7 | runtime-profile | P1_readonly_candidate | process-pool | 4 | 1 | clientInstanceId, sessionId, projectRoot | credential-or-provider-budget:token-bucket | no | metered |
| git | runtime-profile | P3_project_safe | project-pool | 4 | 1 | projectRoot, sessionId, clientInstanceId | repo:write | no | free |
| playwright | runtime-profile | PX_forbidden_browser_until_context_isolated | session-pool | 2 | 1 | browserContextId, sessionId, clientInstanceId, transportSessionId | browser-context:exclusive, session:exclusive | yes | free |
| unknown-stdio-npx | edge-fixture | P0_unknown_stdio | process-pool | 2 | 1 | clientInstanceId, sessionId, projectRoot | server:exclusive | yes | unknown |
| legacy-sse | edge-fixture | PX_legacy_compat | legacy-disabled | 0 | 0 | none | legacy-transport:exclusive | yes | free |
| remote-streamable-http | edge-fixture | P4_stateless_remote_candidate | remote-http-session-pool | 8 | 4 | transportSessionId, sessionId, credentialProfile, tenantId | credential-or-provider-budget:token-bucket | no | metered |
| credential-scoped-api | edge-fixture | P2_session_safe | credential-session-pool | 4 | 1 | credentialProfile, sessionId, tenantId | credential:oauth-subject:exclusive | no | metered |
| project-filesystem-write | edge-fixture | P3_project_safe | project-pool | 4 | 1 | projectRoot, sessionId, clientInstanceId | project:write, file:write | no | free |
| repo-git-write | edge-fixture | P3_project_safe | project-pool | 4 | 1 | projectRoot, sessionId, clientInstanceId | repo:write | no | free |
| browser-automation | edge-fixture | PX_forbidden_browser_until_context_isolated | session-pool | 2 | 1 | browserContextId, sessionId, clientInstanceId, transportSessionId | browser-context:exclusive, session:exclusive | yes | free |
| shared-exclusive-desktop | edge-fixture | PX_forbidden | singleton | 1 | 1 | tenantId | session:exclusive | yes | free |
| readonly-stdio-candidate | edge-fixture | P1_readonly_candidate | process-pool | 4 | 1 | clientInstanceId, sessionId, projectRoot | credential-or-provider-budget:token-bucket | no | metered |
| oci-unknown | edge-fixture | P0_unknown_stdio | process-pool | 2 | 1 | clientInstanceId, sessionId, projectRoot | server:exclusive | yes | unknown |

## Invariants

- PASS profiles-materialized: Runtime server profiles resolve to worker plans.
- PASS edge-cases-materialized: Synthetic adaptive edge cases resolve to worker plans.
- PASS unknown-safe: Unknown servers remain one in-flight per worker.
- PASS legacy-disabled: Legacy transports are disabled for worker scheduling.
- PASS affinity-boundaries: Every worker plan has required affinity, lock, consent, and degradation boundaries.

## Warnings

- context7: budget/rate-limit guardrail must be enforced at runtime.
- playwright: consent/review gate remains required before risky execution.
- unknown-stdio-npx: generated conservative plan; safe probes required before raising concurrency.
- unknown-stdio-npx: consent/review gate remains required before risky execution.
- legacy-sse: consent/review gate remains required before risky execution.
- remote-streamable-http: budget/rate-limit guardrail must be enforced at runtime.
- credential-scoped-api: budget/rate-limit guardrail must be enforced at runtime.
- browser-automation: consent/review gate remains required before risky execution.
- shared-exclusive-desktop: consent/review gate remains required before risky execution.
- readonly-stdio-candidate: budget/rate-limit guardrail must be enforced at runtime.
- oci-unknown: generated conservative plan; safe probes required before raising concurrency.
- oci-unknown: consent/review gate remains required before risky execution.

