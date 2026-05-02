# Product practice guardrails

The project should not grow features faster than proof.

Current allowed claims:

- Source tree and thin npm launcher can be evaluated locally.
- Useful MCP setup is preset-first and data-driven.
- Local stdio upstream MCP is the primary callable upstream path.
- Local runtime trace can be claimed when `reports/runtime-trace-latest.json`
  has status `pass`.

Current disallowed claims until proof exists:

- Published binary install readiness.
- Universal remote Streamable HTTP upstream broker.
- Strict durable multi-client/session isolation.

Run:

```bash
npm run verify:product-practice
npm run verify:runtime-trace
```

The practice harness checks that source health, runtime proof, and published install proof stay separate. This matters because a project can look finished after adding commands, docs, and reports while the actual broker loop remains unproven.

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

Until that trace exists with status `pass` in `reports/runtime-trace-latest.json`, MCPace should be described as source/thin-launcher ready with warnings, not runtime beta ready. Even after it passes, do not claim published binary install readiness until a native binary or platform package is staged and verified.
