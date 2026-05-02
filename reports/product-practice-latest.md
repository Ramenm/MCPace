# MCPace product-practice harness

Project: `mcpace` v`0.5.9`
Status: `prove-runtime-before-more-features`

## Claims

| claim | allowed |
|---|---:|
| sourceTreeHealthy | yes |
| sourceThinLauncherInstall | yes |
| runtimeBeta | no |
| publishedBinaryInstall | no |
| universalRemoteMcpBroker | no |

## Gates

| gate | status | evidence |
|---|---:|---|
| source-inventory | pass | inventory ok |
| node-syntax | pass | 80/80 JS/MJS files checked |
| lint-hardcode | pass | node scripts/check-node-syntax.mjs --json |
| rust-build | pass | pass |
| runtime-trace | blocked | ready-to-run |
| published-binary-install | blocked | ready-with-warnings |

## Wrong-practice risks

- Feature accumulation can make the project feel done before the actual broker loop is proven.
- Thin npm launcher install can be useful, but it is not the same as published native binary install.

## Next moves

- Capture runtime trace: client -> /mcp -> tools/list -> tools/call -> stdio upstream trace.
- Stage and verify at least one native binary/platform package before claiming published install readiness.
