# Load-test report

This source bundle includes `scripts/load-test-local.mjs` for local runtime smoke and load validation.

Validated on Windows with the release binary:

```bash
npm run load:local -- --binary ./target/release/mcpace.exe --duration-ms 5000 --concurrency 64
```

Result: passed.

- `/healthz`: 10,358 requests, 0 failed.
- `/api/overview`: 2,228 requests, 0 failed.
- `/mcp` initialize POST: 11,667 requests, 0 failed.
- Edge probes passed: spoofed Host rejection, cross-origin MCP POST rejection, missing Streamable HTTP Accept rejection, over-limit body rejection, and unknown session rejection.

The script starts MCPace against an isolated temporary root, checks `/healthz`, `/api/overview`, and `/mcp`, and verifies key HTTP/MCP guardrails such as host validation, CORS handling, Streamable HTTP `Accept` validation, oversized body rejection, and unknown session id rejection.
