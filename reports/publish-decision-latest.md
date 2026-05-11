# Publish decision
Generated: 2026-05-04T13:22:39.048Z
Project: mcpace 0.5.9
Status: **source-ready-publish-blocked**
Public source snapshot: **allowed**
npm/native publication: **blocked**
Paid GitHub plan required: **no**.
| Gate | Scope | Status | Evidence |
|---|---|---:|---|
| local-quality-source | source | pass | reports/local-quality-source-latest.json: pass-with-warnings, 3.8h old, max 6h |
| secret-scan | source | pass | reports/secret-scan-latest.json: pass, 3.5h old, max 6h |
| supply-chain-risk | source | warning | reports/supply-chain-risk-latest.json: pass-with-warnings, 3.5h old, max 6h |
| free-tier-readiness | source | pass | reports/free-tier-readiness-latest.json: ready, 3.5h old, max 6h |
| product-practice-source | source | pass | reports/product-practice-latest.json: ready-for-release-candidate-review, 0h old, max 6h |
| rust-quality | release | pass | reports/rust-quality-latest.json: pass, 0h old, max 6h |
| runtime-trace | release | pass | reports/runtime-trace-latest.json: pass, 0h old, max 6h |
| vendored-binary | release | pass | reports/vendored-binary-latest.json: pass, 0.1h old, max 6h |
| local-prepublish | release | blocked | reports/local-prepublish-latest.json: pass-with-warnings, 0h old, max 6h |
| product-practice-release | release | pass | runtimeBeta=true, publishedBinaryInstall=true |

## Next actions

- Run npm run verify:local-prepublish on the release host.
- Review supply-chain warnings before a polished launch.