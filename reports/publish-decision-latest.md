# Publish decision
Generated: 2026-05-17T15:35:09.020Z
Project: mcpace 0.6.5
Status: **source-ready-publish-blocked**
Public source snapshot: **allowed**
npm/native publication: **blocked**
Paid GitHub plan required: **no**.
| Gate | Scope | Status | Evidence |
|---|---|---:|---|
| local-quality-source | source | pass | reports/local-quality-source-latest.json: pass-with-warnings, 0h old, max 6h |
| secret-scan | source | pass | reports/secret-scan-latest.json: pass, 0h old, max 6h |
| supply-chain-risk | source | warning | reports/supply-chain-risk-latest.json: pass-with-warnings, 0h old, max 6h |
| free-tier-readiness | source | warning | reports/free-tier-readiness-latest.json: ready-with-warnings, 0h old, max 6h |
| product-practice-source | source | pass | reports/product-practice-latest.json: prove-rust-before-runtime-claims, 0h old, max 6h |
| rust-quality | release | blocked | reports/rust-quality-latest.json: partial, 0h old, max 6h |
| runtime-trace | release | pass | reports/runtime-trace-latest.json: pass, 0.1h old, max 6h |
| vendored-binary | release | pass | reports/vendored-binary-latest.json: pass, 0.1h old, max 6h |
| local-prepublish | release | blocked | reports/local-prepublish-latest.json missing |
| product-practice-release | release | blocked | runtimeBeta=false, publishedBinaryInstall=true |

## Next actions

- Run npm run verify:rust-quality on a host with Cargo dependency access.
- Run npm run verify:local-prepublish on the release host.
- Get fresh Rust, vendored binary, and runtime-trace proof before claiming release readiness.
- Review supply-chain warnings before a polished launch.
- Keep local/free-tier proof path intact.