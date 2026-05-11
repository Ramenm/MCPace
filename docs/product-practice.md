# Product practice guardrails

The project should not grow features faster than proof.

Current allowed claims:

- Source tree and thin npm launcher can be evaluated locally.
- Useful MCP setup is preset-first and data-driven.
- Local stdio upstream MCP is the primary callable upstream path.
- Local runtime trace evidence can be accepted when `reports/runtime-trace-latest.json`
  has status `pass`; the stronger runtime beta claim also requires fresh Rust
  quality proof.
- Host-compatible published binary install readiness can be claimed only when
  `reports/vendored-binary-latest.json` or
  `reports/vendored-binary-<target>.json` is fresh, matches the current host
  target, and has status `pass`.

Current disallowed claims until proof exists:

- Published binary install readiness without a fresh vendored-binary proof report.
- Universal remote Streamable HTTP upstream broker.
- Cross-process or relay-grade multi-client/session isolation.

Run:

```bash
npm run verify:product-practice
npm run verify:runtime-trace
```

The practice harness checks that source health, runtime proof, and published install proof stay separate. Proof reports must also be fresh for the current host; the default freshness window is six hours and can be overridden with `--max-report-age-hours` or `MCPACE_MAX_REPORT_AGE_MS` when a release workflow intentionally uses a longer same-run window. This matters because a project can look finished after adding commands, docs, and reports while the actual broker loop remains unproven.

The runtime trace proof is:

```text
real MCP client
→ MCPace /mcp
→ initialize
→ tools/list
→ tools/call
→ stdio upstream MCP server
→ response
```

Until that trace exists with status `pass` in `reports/runtime-trace-latest.json` and Rust quality proof is fresh and passing, MCPace should be described as source/thin-launcher ready with warnings, not runtime beta ready. Even after it passes, do not claim published binary install readiness until a native binary or platform package is staged and `npm run verify:vendored-binary` has produced a fresh passing report for the current host target.
