# MCPace defect gates

Generated: 2026-05-04T13:21:15.413Z

Project: `mcpace` v`0.5.9`

Status: **pass**

| Gate | Status | File |
|---|---|---|
| Bug reports require reproducibility, environment, severity, and regression context | pass | .github/ISSUE_TEMPLATE/bug_report.yml |
| Repository has a label taxonomy for severity, area, type, and status triage | pass | .github/labels.yml |
| Maintainers have a written bug lifecycle and fix standard | pass | docs/bug-lifecycle.md |
| Pull requests require proof for fixes and explicit not-tested disclosure | pass | .github/pull_request_template.md |
| CI runs defect gates alongside source and GitHub readiness checks | pass | .github/workflows/ci.yml |
| npm script exposes the defect gate as a first-class verification command | pass | package/script |
| MCP HTTP initialize generates server-owned session ids | pass | src/dashboard/mcp_http.rs |
| Local browser-origin guard rejects null Origin instead of treating it as local | pass | src/dashboard/http_boundary.rs |
| Local HTTP mode refuses non-loopback bind hosts unless explicitly opted in | pass | src/dashboard.rs |
| Security issues have a private disclosure path and public policy | pass | SECURITY.md |
| Source audit remains available for structural bug smells | pass | scripts/audit-source.mjs |
| Runtime trace gate exists for behavioral bugs beyond static checks | pass | scripts/runtime-trace-harness.mjs |
| Product-practice gate rejects stale runtime proof | pass | scripts/product-practice-harness.mjs |

## Operating model

- Intake: Every bug starts with a reproducible issue, severity, affected area, version/platform, and expected-vs-actual behavior.
- Repair: Every fix gets a minimal failing test or runtime trace first, then root-cause notes, implementation, regression guard, and not-tested disclosure.
- Release: Bugfix releases are accepted only after source gates, Rust gates, runtime trace, npm/install readiness, and security gates are fresh.
