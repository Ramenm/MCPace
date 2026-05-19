# MCPace local quality suite
Project: `mcpace` v`0.6.5`
Profile: `source`
Status: `pass-with-warnings`
GitHub paid plan required: `no`
## Decision
Public source snapshot is allowed; native/npm runtime publication still needs full/release proof.
## Steps
| group | step | required | status | evidence |
|---|---|---:|---:|---|
| tooling | toolbox-doctor | no | warn | ready-with-warnings |
| source | node-syntax | yes | pass | 162 checked |
| source | source-audit | yes | pass | pass |
| lifecycle | system-lifecycle-audit | yes | pass | pass |
| runtime-scale | mixed-upstream-topology | yes | pass | pass |
| runtime-scale | upstream-failsafe | yes | pass | pass |
| runtime-safety | tool-exposure-safety | yes | pass | pass |
| runtime-safety | tool-message-integrity | yes | pass | pass |
| quality | defect-gates | yes | pass | pass |
| quality | bug-sweep | yes | pass | pass |
| security | secret-scan | yes | pass | pass |
| security | supply-chain-risk | no | warn | pass-with-warnings |
| public-surface | github-health | no | pass | pass |
| public-surface | github-readiness | no | warn | ready-with-warnings |
| public-surface | free-tier-readiness | yes | pass | ready |
| package | install-readiness-source | no | pass | ready |
| tests | repo-node-smoke-tests | yes | pass | pass |
| tests | npm-cli-tests | yes | pass | pass |
| rust | cargo-metadata | yes | pass | pass |
| rust | cargo-fmt | yes | pass | exit 0 |
| package | npm-pack | yes | pass | pass |
| package | platform-package-manifests | yes | pass | pass |
| claims | product-practice | no | pass | ready-for-release-candidate-review |

## Next actions

- toolbox-doctor: ready-with-warnings
- supply-chain-risk: pass-with-warnings
- github-readiness: ready-with-warnings
