# MCPace local pre-publish gate
Project: `mcpace` v`0.6.5`
Mode: `quick`
Status: `pass-with-warnings`
GitHub paid plan required: `no`
## Decision
No required blockers, but warnings should be resolved before a polished public launch.
## Steps
| group | step | required | status | evidence |
|---|---|---:|---:|---|
| toolchain | tooling-readiness | yes | warn | ready-with-warnings |
| source | node-syntax | yes | pass | 162 checked |
| source | source-audit | yes | pass | exit 0 |
| quality | defect-gates | yes | pass | pass |
| quality | bug-sweep | yes | pass | pass |
| public-surface | public-repo-health | no | pass | pass |
| public-surface | public-readiness | no | warn | ready-with-warnings |
| package | npm-thin-pack | yes | pass | pass |
| package | platform-package-manifests | yes | pass | pass |
| rust | cargo-metadata | yes | pass | exit 0 |
| rust | cargo-fmt | yes | pass | exit 0 |

## Next actions

- tooling-readiness: ready-with-warnings
- public-readiness: ready-with-warnings
