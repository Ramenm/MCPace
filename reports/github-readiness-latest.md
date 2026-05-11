# MCPace GitHub readiness

Project: `mcpace` v`0.5.9`
Status: `ready-with-warnings`

## Summary

- Required checks: `24/24`
- Advisory warnings: `1`
- Issue templates: `bug_report.yml, cleanup-request.yml, client-upstream-compatibility.yml, config.yml, docs_request.yml, feature_request.yml, flaky-test.yml, repair-report.yml, runtime-proof.yml`

## Checks

| check | required | status | evidence |
|---|---:|---:|---|
| file:README.md | yes | pass | public landing page; 196 lines |
| file:LICENSE | yes | pass | license clarity; 109 lines |
| file:CONTRIBUTING.md | yes | pass | contribution workflow; 110 lines |
| file:SECURITY.md | yes | pass | private vulnerability reporting; 61 lines |
| file:CODE_OF_CONDUCT.md | yes | pass | community behavior rules; 27 lines |
| file:SUPPORT.md | yes | pass | support boundaries and issue routing; 44 lines |
| file:CODEOWNERS | yes | pass | review ownership; 11 lines |
| file:.github/pull_request_template.md | yes | pass | review discipline; 51 lines |
| file:.github/dependabot.yml | yes | pass | dependency update automation; 26 lines |
| file:.github/ISSUE_TEMPLATE/bug_report.yml | yes | pass | bug reports with repro and redacted logs; 125 lines |
| file:.github/ISSUE_TEMPLATE/feature_request.yml | yes | pass | feature requests tied to product proof; 56 lines |
| file:.github/ISSUE_TEMPLATE/runtime-proof.yml | yes | pass | community runtime proof submissions; 63 lines |
| file:ROADMAP.md | yes | pass | public roadmap without overclaiming; 40 lines |
| file:docs/github-launch-playbook.md | yes | pass | public launch operating plan; 225 lines |
| file:docs/runtime-beta-roadmap.md | yes | pass | runtime beta acceptance criteria; 49 lines |
| file:docs/product-truth-and-beta-gate.md | yes | pass | truth taxonomy and beta/GA gates; 173 lines |
| workflow:.github/workflows/ci.yml | yes | pass | normal source, Rust, launcher, and hosted proof CI; 311 lines |
| workflow:.github/workflows/release.yml | yes | pass | draft GitHub Release and platform artifact proof; 276 lines |
| workflow:.github/workflows/publish-npm.yml | yes | pass | npm trusted-publishing lane from release artifacts; 67 lines |
| workflow:.github/workflows/security.yml | yes | pass | supply-chain security review lanes; 116 lines |
| workflow:.github/workflows/codeql.yml | yes | pass | CodeQL scan for the JavaScript/launcher surface; 49 lines |
| script:verify:github-readiness | yes | pass | node scripts/verify-github-readiness.mjs --json --write reports/github-readiness-latest.json --markdown reports/github-readiness-latest.md |
| truthful-readme-claims | yes | pass | README keeps runtime/session/HTTP-upstream limitations explicit. |
| issue-template-set | yes | pass | bug_report.yml, cleanup-request.yml, client-upstream-compatibility.yml, config.yml, docs_request.yml, feature_request.yml, flaky-test.yml, repair-report.yml, runtime-proof.yml |
| file:FUNDING.yml | no | warn | missing |
| file:CITATION.cff | no | pass | academic/software citation metadata if MCPace becomes research-facing; 9 lines |

## Next moves

- Add FUNDING.yml when this project needs funding/sponsorship links once the project is public.
