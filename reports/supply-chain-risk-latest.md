# MCPace local supply-chain risk audit
Project: `mcpace` v`0.5.9`
Status: `pass-with-warnings`
GitHub paid plan required: `no`
| check | severity | status | evidence |
|---|---:|---:|---|
| root-package-no-runtime-deps | required | pass | root package has no runtime dependencies |
| root-package-no-dev-deps | advisory | pass | root package has no devDependencies; scripts use built-in Node/Rust tooling |
| cli-launcher-no-runtime-deps | required | pass | thin npm launcher has no runtime dependencies |
| platform-optional-dependencies | required | pass | 6 platform optional dependencies |
| cargo-lock-present | required | pass | Cargo.lock present |
| package-lock-absent-ok | advisory | pass | no package-lock.json because the workspace has no npm deps |
| cargo-audit | recommended | warn | cargo-audit not installed/available |
| cargo-deny | recommended | warn | cargo-deny not installed/available |
| gitleaks | optional | warn | gitleaks not installed/available |
| osv-scanner | optional | warn | osv-scanner not installed/available |
| trivy | optional | warn | trivy not installed/available |

## Next actions

- Install cargo-audit and run cargo audit before public release.
- Install cargo-deny for license/source/advisory policy before public release.
- Install gitleaks for an independent local secret scan.
- Install osv-scanner for an independent vulnerability scan.
- Install trivy if you want container/filesystem vulnerability scans.