# MCPace free-tier/local-first readiness

Project: `mcpace` v`0.5.9`
Status: `ready`
Paid GitHub required: `no`

| gate | status | evidence |
|---|---:|---|
| local-first-package-scripts | pass | 9 local/free-tier scripts present. |
| local-first-docs | pass | Local-first, no-paid-GitHub, and release decision docs are present. |
| readme-local-first | pass | README explains local source proof and paid GitHub is not required. |
| github-workflows-optional | pass | 6 workflows present; local scripts remain source of truth. |
| no-long-lived-npm-token-required | pass | No required NPM_TOKEN workflow dependency detected. |
| trusted-publishing-shape | pass | OIDC/trusted-publishing shape is present or documented. |
| local-proof-reports-present | pass | 4 local/free-tier reports present. |

## Policy

- Local scripts and generated reports prove source/package/runtime readiness before GitHub mirrors it.
- A public repository can use free hosted GitHub Actions/security features, but release decisions should not require a paid plan.
