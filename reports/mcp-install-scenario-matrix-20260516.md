# MCP install scenario matrix

| Area | Scenario | Expected behavior | Current evidence | Residual risk |
|---|---|---|---|---|
| Registration | `server install filesystem --dry-run` | No file is written; output says `dry-run-add`. | Covered by `npm run verify:mcp-install-scenarios`. | Does not prove runtime package launch. |
| Idempotency | Install same preset twice | Second run fails unless `--force` is used. | Covered by executable smoke. | Users may expect `install` to update package versions; docs must say it updates config only. |
| Replacement | Install with `--force` | Existing fragment is replaced. | Covered by executable smoke. | Force can overwrite local manual edits. |
| Transport types | `stdio` and `streamable-http` | Correct type and command/url are preserved. | Covered by executable smoke. | Client transport support varies by client. |
| Remote domains | `--url https://...` | URL is stored as upstream URL. | Covered by executable smoke. | MCPace does not verify domain ownership or provider trust. |
| Invalid URL | `ssh://...` | Rejected. | Covered by executable smoke. | Does not validate live HTTP auth or TLS properties. |
| Paid servers | Add with `--disabled` | Config exists but server is disabled. | Covered by executable smoke. | Billing can still happen after later enable/tool call. |
| Scale | 100 configured servers | 100 fragments can be written and inventoried. | Covered by executable smoke. | Does not prove 100 concurrent live processes are safe. |
| Package manager | `npx -y ...` preset | Package execution is deferred until runtime/test. | Covered by fragment inspection. | `npx` may fetch/cache package later depending on local cache and package state. |
| Ownership | `serve.publicUrl` vs upstream URL | Public MCPace endpoint and upstream provider domain remain separate. | Covered by docs/report. | User can misconfigure DNS/relay or point to untrusted domain. |
