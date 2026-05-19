# Release hardening audit — guardrails pass (2026-05-17)

This pass reviewed MCPace as a source/beta hardening branch, not as a final native binary release. The goal was to reduce embarrassing release risk around arbitrary MCP server discovery, supply-chain probing, policy classification, and public claims.

## Fixed in this pass

- Live random MCP probe schema advanced to `mcpace.liveRandomMcpProbe.v5`.
- Package-manager stdout/stderr is redacted before report/log persistence.
- Package-manager commands now resolve npm/uv before switching to a whitelisted PATH, so hardened env does not accidentally make uv/npm disappear.
- Package-manager subprocesses now use capped stdout/stderr capture and a hard-settle timer after SIGTERM/SIGKILL, reducing CI hang and memory blow-up risk.
- Runtime probe wrapper processes now receive a minimal wrapper environment, while third-party runtime commands still run under `/usr/bin/env -i` with a stripped runtime env.
- The probe validates `--ids` and `--kinds` so typos fail loudly instead of producing misleading empty/blocked reports.
- MCP stdout JSON-RPC capture is capped and tracks dropped messages, preventing malicious log/notification floods from growing memory unbounded.
- Additional high-risk policy classes were added: payment/financial, identity-admin, secrets-manager, and messaging/email.
- Registry metadata fixture now covers these high-risk classes and keeps them default-disabled/review-gated.
- Removed a duplicate `routingGroup` key in the registry lab blockchain policy object.

## Real live probes run in this pass

- npm canary sanity: `official-filesystem` + `code-runner` passed, 15 tools discovered, `roots/list` handled once, and `code-runner` stayed `disabled-dangerous-command-runner`.
- PyPI/uv sanity: `python-time` + `python-fetch` passed, 3 tools discovered, and uv remained reachable after executable resolution under the whitelisted env.

## Still blocked before final production binary release

- Rust source changes are not rebuilt in this environment because `cargo`/`rustc` are unavailable.
- Docker/server-container lanes are not proved in this environment.
- Random destructive tool calls are intentionally not executed without a separate sandbox/chroot/container/firejail lane and explicit fixture allowlist.
- Full `npm run test:repo` started and completed batches through 14/26 before the tool call timed out; the batch that appeared next passed when run directly, but a complete single-command full pass was not captured in this environment.
- HTTP upstream relay-grade session pooling and remote-auth policy remain future work; public claims must stay stdio-first/runtime-preview unless fresh proof says otherwise.

## Public wording that is safe

MCPace can discovery-probe pinned npm/PyPI MCP servers, classify risky server families conservatively, and keep unknown/problematic servers review-gated. It does not claim safe destructive execution of arbitrary MCP servers without a separate sandbox and policy review.

## Public wording that is not safe

- MCPace safely supports every random MCP server.
- MCPace can run arbitrary MCP tools safely by default.
- MCPace is production-ready for destructive execution of unknown third-party servers.
- The vendored binary proves newly edited Rust/UI source until it is rebuilt and tested in CI.
