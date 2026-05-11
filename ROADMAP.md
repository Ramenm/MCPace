# Roadmap

MCPace should win by being honest, local-first, easy to install, and boringly reliable. The roadmap is ordered by what makes the product more trustworthy, not by what sounds biggest.

## Now: make the first public release believable

- Keep `serve` as the main product story: one local MCP endpoint, one place to inspect health, one safer way to wire clients.
- Keep BYO upstream MCP servers as the default; do not bundle arbitrary upstream servers into the repo.
- Regenerate fresh proof reports on a supported host before any beta or release claim.
- Ship GitHub-ready community files, issue templates, release notes, support boundaries, and security reporting.
- Stage and verify native binaries for supported npm platform packages.

## Next: runtime beta gate

- Implement and prove HTTP MCP session create/reuse/close semantics.
- Prove real client -> MCPace -> upstream stdio tool call on supported hosts.
- Implement HTTP/Streamable HTTP upstream forwarding with SSRF/auth/timeout protections.
- Strengthen lease ownership beyond single request wrappers: cross-process ownership, stale result guards, cancellation propagation, and backpressure.
- Add compatibility traces for catalog tier-1 local clients.

## Later: broader product surface

- Public relay only after auth, origin, audit logging, and abuse boundaries are designed and tested.
- Team/enterprise policy only after local runtime correctness is stable.
- Musl/Linux Alpine packages only after separate build and install proof.
- More client surfaces only when the catalog proof tier and rollback story are clear.

## Non-goals for the first public launch

- Do not claim universal MCP runtime support.
- Do not claim public cloud relay support.
- Do not silently install or recommend arbitrary upstream MCP servers.
- Do not publish npm packages from long-lived tokens when Trusted Publishing is available.

## Public claim markers

- **Runtime beta** is not the promise yet; it starts only after the in-process session lifecycle has fresh proof, real-client traces pass, and current-target runtime proof is green.
- **Published install** is not the promise yet; it starts only after native binaries, platform packages, checksums, and npm/GitHub release proof pass.
- **Not the promise yet**: universal remote MCP brokering, cloud relay, enterprise policy, and HTTP upstream fan-out before implementation proof.
