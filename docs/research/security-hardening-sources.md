# Security hardening research sources

This note records the authoritative sources used to prioritize MCPace hardening. Links are intentionally kept next to the engineering decisions so future sessions can re-check them as specifications evolve.

## Sources

1. **Model Context Protocol — Transports (2025-11-25)**
   <https://modelcontextprotocol.io/specification/2025-11-25/basic/transports>
   Relevant for Origin validation, localhost binding, session identifiers, Streamable HTTP, cancellation, and resumability.

2. **Model Context Protocol — Security Best Practices (2025-11-25)**
   <https://modelcontextprotocol.io/specification/2025-11-25/basic/security_best_practices>
   Relevant for per-request authorization, binding sessions to users, SSRF defenses, local-server sandboxing, and least privilege.

3. **Model Context Protocol — Tools (2025-11-25)**
   <https://modelcontextprotocol.io/specification/2025-11-25/server/tools>
   Relevant for tool-list change notifications, pagination, output-schema validation, human confirmation, and untrusted annotations.

4. **OWASP Server-Side Request Forgery Prevention Cheat Sheet**
   <https://cheatsheetseries.owasp.org/cheatsheets/Server_Side_Request_Forgery_Prevention_Cheat_Sheet.html>
   Relevant for private/link-local address blocking, DNS-rebinding resistance, redirect handling, and egress allow-lists.

5. **GitHub Docs — Dependabot configuration options**
   <https://docs.github.com/en/code-security/dependabot/dependabot-version-updates/configuration-options-for-the-dependabot.yml-file>
   Relevant because `open-pull-requests-limit: 0` disables version-update pull requests.

6. **GitHub Docs — Artifact attestations**
   <https://docs.github.com/en/actions/security-guides/using-artifact-attestations-to-establish-provenance-for-builds>
   Relevant for release provenance and signed SBOM attestations.

7. **The Rustup Book — Installation**
   <https://rust-lang.github.io/rustup/installation/index.html>
   Relevant for project-local/user-local Rust toolchain setup and toolchain selection.

8. **OpenTelemetry — Observability primer**
   <https://opentelemetry.io/docs/concepts/observability-primer/>
   Relevant for correlating logs, metrics, and traces across the dashboard, MCP transport, and upstream processes.

## Comparison matrix

| Option | Implementation | Maintenance | Risks | Performance | Compatibility | License / cost |
|---|---|---|---|---|---|---|
| Optional bearer token on loopback | Low | Low | Local processes and browser-based attacks share the trust zone | High | High | No added cost |
| Mandatory bearer token for all HTTP operations | Medium | Medium | Browser bootstrap and token handling must be designed safely | High | High for API clients; UI migration required | No added cost |
| Read-only unauthenticated dashboard + authenticated mutation plane | Medium | Medium | Route classification must be exhaustive and fail closed | High | High | No added cost |
| OS-protected Unix socket / named pipe | Medium–high | Medium | Cross-platform implementation complexity | High | Strong for local clients; weaker browser compatibility | No added cost |
| Caller-supplied risk grants | Low | Low | Self-authorization and prompt/tool injection | High | Existing clients | No added cost |
| Server-issued, principal-bound one-time approval receipts | High | Medium | State-machine and replay bugs if implemented incompletely | High | Requires client/UI migration | No added cost |
| URL deny-list only | Low | High | Incomplete against DNS rebinding and new special ranges | High | High | No added cost |
| Central egress policy + resolved-IP validation + no redirects | Medium–high | Medium | DNS pinning and proxy operational complexity | Medium–high | May reject previously accepted endpoints | Proxy cost depends on deployment |

## Recommendation for MCPace

1. Keep loopback binding and Origin/Host validation, but separate the read-only presentation surface from a fail-closed authenticated mutation plane.
2. Replace caller-supplied authorization grants with short-lived, one-time approval receipts bound to authenticated principal, exact tool, exact argument digest, and expiry.
3. Introduce one central outbound-target policy that validates every resolved address, rejects private/link-local/metadata destinations by default, and re-validates redirects if redirects are ever enabled.
4. Bind HTTP sessions and leases to authenticated principals; generate all bearer-like identifiers with the OS CSPRNG.
5. Preserve current release attestations and add signed SBOM attestations after the build is reproducible.

**Plan B:** if a full identity/approval migration cannot land atomically, disable HTTP mutations when no token is configured and retain direct stdio operation as the compatibility path.

**Critical checks before final architectural choice:** browser authentication flow, Windows named-pipe ACL semantics, Unix socket permissions, multi-principal adversarial tests, DNS-rebinding tests, cancellation propagation, and backward compatibility for existing MCP clients.

Revisit the choice if MCP standard authorization/session guidance changes, if browser clients gain a safer local-IPC bridge, or if MCPace becomes remotely bindable.
