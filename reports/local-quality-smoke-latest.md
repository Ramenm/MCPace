# MCPace local quality suite
Project: `mcpace` v`0.5.9`
Profile: `smoke`
Status: `pass-with-warnings`
GitHub paid plan required: `no`
## Decision
No required blockers, but warnings remain.
## Steps
| group | step | required | status | evidence |
|---|---|---:|---:|---|
| source | node-syntax | yes | pass | 99 checked |
| source | source-audit | yes | pass | pass |
| quality | defect-gates | yes | pass | pass |
| quality | bug-sweep | yes | warn | 1 warnings |
| security | secret-scan | yes | pass | pass |

## Next actions

- bug-sweep: 1 warnings