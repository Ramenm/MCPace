# MCPace local pre-publish gate
Project: `mcpace` v`0.5.9`
Mode: `full`
Status: `pass-with-warnings`
GitHub paid plan required: `no`
## Decision
No required blockers, but warnings should be resolved before a polished public launch.
## Steps
| group | step | required | status | evidence |
|---|---|---:|---:|---|
| toolchain | tooling-readiness | yes | warn | ready-with-warnings |
| source | node-syntax | yes | pass | 99 checked |
| source | source-audit | yes | pass | exit 0 |
| quality | defect-gates | yes | pass | pass |
| quality | bug-sweep | yes | pass | pass |
| public-surface | public-repo-health | no | pass | pass |
| public-surface | public-readiness | no | warn | ready-with-warnings |
| package | npm-thin-pack | yes | pass | pass |
| package | platform-package-manifests | yes | pass | pass |
| rust | cargo-metadata | yes | pass | exit 0 |
| rust | cargo-fmt | yes | pass | exit 0 |
| rust | rust-quality-full | yes | pass | pass |
| package | vendored-binary | yes | pass | pass |
| runtime | runtime-trace | yes | pass | pass |
| package | install-readiness | yes | pass | ready |
| release-claim | product-practice | yes | pass | ready-for-release-candidate-review |

## Next actions

- tooling-readiness: ready-with-warnings
- public-readiness: ready-with-warnings