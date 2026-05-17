# Adaptive parallelism audit

Status: **pass**
Generated: 2026-05-17T15:29:13.177Z

## Summary

- Profiles inspected: 4
- Edge-case fixtures: 10
- Stable/default profiles: 3
- Conservative/unknown profiles: 0
- Legacy compatibility profiles: 0
- Blockers: 0
- Warnings: 1

## Runtime/config profiles

| Server | Source | Transport | Launcher | Safety class | Pool | Workers | In-flight/worker | Lock domains |
|---|---|---|---|---|---|---:|---:|---|
| filesystem | preset | stdio | npx | P3_project_safe | project-pool | 4 | 1 | project, file |
| context7 | preset | stdio | npx | P1_readonly_candidate | process-pool | 4 | 1 | credential-or-provider-budget |
| git | preset | stdio | uvx | P3_project_safe | project-pool | 4 | 1 | repo |
| playwright | preset | stdio | npx | PX_forbidden_browser_until_context_isolated | session-pool | 2 | 1 | browser-context, session |

## Edge-case matrix

| Case | Expected | Actual | Status | Rationale |
|---|---|---|---|---|
| unknown-stdio-npx | P0_unknown_stdio/process-pool/1 | P0_unknown_stdio/process-pool/1 | PASS | Unknown stdio can scale only through isolated workers; a single worker stays one in-flight until probes pass. |
| legacy-sse | PX_legacy_compat/legacy-disabled/0 | PX_legacy_compat/legacy-disabled/0 | PASS | Legacy SSE compatibility must not be folded into stable Streamable HTTP scheduling. |
| remote-streamable-http | P4_stateless_remote_candidate/remote-http-session-pool/4 | P4_stateless_remote_candidate/remote-http-session-pool/4 | PASS | Remote Streamable HTTP can use session/provider budgets, not local stdio process assumptions. |
| credential-scoped-api | P2_session_safe/credential-session-pool/1 | P2_session_safe/credential-session-pool/1 | PASS | Credential identity is a scheduling boundary even when the launcher is local stdio. |
| project-filesystem-write | P3_project_safe/project-pool/1 | P3_project_safe/project-pool/1 | PASS | Project-local file tools require project/file lock domains; worker concurrency comes from isolation. |
| repo-git-write | P3_project_safe/project-pool/1 | P3_project_safe/project-pool/1 | PASS | Git/repository tools can parallelize across repos but must serialize conflicting repo writes. |
| browser-automation | PX_forbidden_browser_until_context_isolated/session-pool/1 | PX_forbidden_browser_until_context_isolated/session-pool/1 | PASS | Browser automation needs browser-context/session isolation before parallel scheduling. |
| shared-exclusive-desktop | PX_forbidden/singleton/1 | PX_forbidden/singleton/1 | PASS | Desktop/host-global state stays singleton unless a stronger isolation key is proven. |
| readonly-stdio-candidate | P1_readonly_candidate/process-pool/1 | P1_readonly_candidate/process-pool/1 | PASS | Read-heavy stdio stays one in-flight per worker until safe probes prove higher concurrency. |
| oci-unknown | P0_unknown_stdio/process-pool/1 | P0_unknown_stdio/process-pool/1 | PASS | Container launchers are still untrusted upstream code until classified/probed. |

## Checks

- PASS server-profile-fields: ServerRecord exposes adaptive profile fields.
- PASS source-type-normalization: Legacy SSE is separated from stable Streamable HTTP.
- PASS client-plan-scheduling: Client routing plan includes adaptive worker-pool planning and probe-gated fallback.
- PASS schema-profile: Server profile schema exists.
- PASS schema-worker: Worker plan schema exists.
- PASS docs: Adaptive orchestration architecture doc exists.
- PASS edge-case-docs: Adaptive edge-case coverage doc exists.
- PASS no-legacy-default: Legacy transport is never auto-parallelized.
- PASS unknown-is-conservative: Unknown profiles are maxInFlightPerWorker=1.
- PASS edge-case-matrix: Synthetic edge-case matrix covers unknown, legacy, remote, credential, project, repo, browser, desktop, readonly, and OCI classifications.
- PASS edge-locks-present: Every edge-case classification carries at least one lock or scheduling domain.

## Warnings
- playwright: high-risk/legacy profile requires explicit policy before parallelism.
