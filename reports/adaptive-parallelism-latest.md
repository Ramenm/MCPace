# Adaptive MCP parallelism audit

Generated: 2026-05-19T12:36:08.242Z
Status: **pass**

Configured profiles: 0; edge cases: 13; static catalogs present: no.

## Profiles

| Server | Source | Transport | Launcher | Safety | Pool | Workers | Locks | Stateless |
|---|---|---|---|---|---|---:|---|---:|
| none | empty by default | - | - | - | - | 0 | - | - |

## Edge cases

- PASS unknown-stdio-npx: Unknown stdio remains one in-flight until probes and policy evidence exist.
- PASS legacy-sse: Legacy SSE is not treated as modern Streamable HTTP scheduling.
- PASS remote-streamable-http: Remote HTTP is session-bound until MCP-Session-Id/probe evidence proves otherwise.
- PASS stateless-remote-http: Only explicit stateless evidence raises remote HTTP to broad fan-out.
- PASS credential-scoped-api: Credential/API surfaces need profile or tenant affinity.
- PASS project-filesystem-write: Filesystem tools lock project/file domains.
- PASS repo-git-write: Git tools lock repo/project domains.
- PASS browser-automation: Browser automation cannot fan out without browser-context isolation.
- PASS shared-exclusive-desktop: Desktop/profile control is shared-exclusive.
- PASS readonly-stdio-candidate: Small local utilities can become multi-reader after explicit read-only evidence.
- PASS stateful-memory: Memory/context stores are session/profile stateful.
- PASS local-database: Local databases lock database/project domains.
- PASS oci-unknown: Container images remain unknown until provenance and probes are reviewed.

## Checks

- PASS no-packaged-upstream-catalog: Packaged upstream-server catalogs are absent; install/profile behavior is evidence-first.
- PASS auto-profile-config: mcpace.config.json documents automatic profiling and no longer exposes static server catalogs.
- PASS no-bundled-default-upstreams: The project ships with no enabled upstream MCP servers by default.
- PASS edge-case-matrix: Synthetic state/session/client edge cases classify to expected conservative plans.
- PASS unknown-is-conservative: Unknown stdio stays one in-flight and review-gated.
- PASS remote-session-default: Remote Streamable HTTP remains session-safe until stateless evidence exists.
- PASS live-probe-harness: Random/live MCP package probe harness exists for package-derived evidence.

## Warnings

- No configured upstream MCP servers in the source snapshot; only synthetic edge cases are profiled here.
