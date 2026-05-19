# MCPace GitHub health audit

Project: `mcpace` v`0.6.5`
Status: `pass`
Score: `100%`

## Checks

| check | category | status | evidence |
|---|---|---:|---|
| file:README.md | community | pass | present |
| file:LICENSE | community | pass | present |
| file:CONTRIBUTING.md | community | pass | present |
| file:SECURITY.md | community | pass | present |
| file:SUPPORT.md | community | pass | present |
| file:CODE_OF_CONDUCT.md | community | pass | present |
| file:CHANGELOG.md | community | pass | present |
| file:ROADMAP.md | community | pass | present |
| file:CODEOWNERS | community | pass | present |
| file:.github/pull_request_template.md | community | pass | present |
| file:.github/dependabot.yml | security | pass | present |
| file:.github/release.yml | release | pass | present |
| file:.github/workflows/ci.yml | automation | pass | present |
| file:.github/workflows/release.yml | automation | pass | present |
| file:.github/workflows/publish-npm.yml | automation | pass | present |
| file:docs/github-launch-playbook.md | docs | pass | present |
| file:docs/product-truth-and-beta-gate.md | docs | pass | present |
| file:docs/release-automation.md | docs | pass | present |
| readme:first-working-path | docs | pass | README exposes a fast first path. |
| readme:byo-upstream | docs | pass | README states BYO upstream model. |
| readme:honest-http-upstream | docs | pass | README does not overclaim HTTP upstream fan-out. |
| security:private-reporting | security | pass | SECURITY.md directs private reporting. |
| support:redaction | security | pass | SUPPORT.md tells users to redact secrets. |
| release:trusted-publishing | release | pass | publish workflow is OIDC/trusted-publishing shaped. |
| release:prebuilt-artifacts | release | pass | publish workflow verifies prebuilt release artifacts before npm publish. |
| release:checksums | release | pass | release workflow generates checksum assets. |
| proof:product-practice | proof | pass | package scripts expose product-practice proof gate. |
| proof:runtime-trace | proof | pass | package scripts expose runtime trace proof gate. |
| proof:github-health | proof | pass | package scripts expose GitHub launch health audit. |
| issues:template-count | community | pass | 8 issue templates |
