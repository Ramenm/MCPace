# Critical hardening audit addendum — 2026-05-17

This addendum records the second realistic random-MCP pass after the initial npm/PyPI probe work.

## What changed

- Live random MCP probe schema is now `mcpace.liveRandomMcpProbe.v3`.
- Probe reports now distinguish `unexpectedFailures` from allowed non-OK outcomes, so credentialed/startup-blocked canaries do not hide real policy mismatches.
- The probe records server-side JSON-RPC requests handled during discovery (`roots/list`, `ping`, rejected unknown methods).
- Additional canaries were added for ESLint, Kubernetes, code execution, OpenAPI bridges, Tavily, Mapbox, BrowserStack, and SAP/Fiori.
- `code-runner` exposed a real classifier bug: the package returned a generic `run-code` tool and was first misclassified as `unknown-conservative-review`. The classifier now detects run-code/code-snippet/language execution surfaces and maps them to `disabled-dangerous-command-runner`.
- `kubernetes-flux159` successfully listed 23 tools and maps to `cluster-admin-credential-review`.
- `openapi-mcp` failed closed because no OpenAPI spec was supplied, but the package is now classified as `network-openapi-review`.
- `mapbox` package installation exceeded the restricted mirror timeout and is now hard-skipped by default unless `--allow-heavy-installs` is explicitly passed.

## Live evidence added

| Report | Result |
|---|---|
| `reports/live-random-mcp-probe-eslint.json` | OK, 1 tool, project-devtools policy |
| `reports/live-random-mcp-probe-code-runner.json` | OK, 1 tool, disabled dangerous command-runner policy |
| `reports/live-random-mcp-probe-kubernetes.json` | OK, 23 tools, cluster-admin credential review policy |
| `reports/live-random-mcp-probe-openapi.json` | expected startup block, OpenAPI spec required |
| `reports/live-random-mcp-probe-tavily.json` | OK, 5 tools, credential-scoped review policy |
| `reports/live-random-mcp-probe-mapbox-hardskip.json` | hard-skipped package-manager canary |

## Critical remaining criticism

1. Random package discovery is now broader, but still discovery-only. It must not call tools outside a destructive sandbox.
2. npm install can still behave badly for large package trees. Known heavy/hanging canaries must remain hard-skipped unless a maintainer explicitly opts in.
3. Tool annotations are treated only as hints. Absence of annotations must never imply safe/read-only behavior.
4. Kubernetes, code-runner, browser, OpenAPI, and credentialed API servers must stay disabled/review-gated by default.
5. Rust rebuild proof is still missing in this environment because `cargo`/`rustc` are unavailable.
6. Docker lane is still missing because Docker is unavailable here.
7. UI source changes still need a native rebuild before claiming the vendored binary includes them.

## Release decision

This archive is acceptable as a source hardening snapshot, not as a final production binary release. A public release should keep the wording conservative: MCPace can classify and discovery-probe random npm/PyPI MCP servers safely, but does not yet prove safe destructive execution.
