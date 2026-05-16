# Playwright parallel session matrix

| Scenario | What it checks | Expected behavior |
|---|---|---|
| Many tabs, one browser context | One user/session opens multiple dashboard tabs | Tabs may share session state, but stale refreshes must not overwrite fresh UI |
| Many clients, many browser contexts | Independent MCP clients/users run at the same time | Each client gets isolated storage, root path, actions, and fixture state |
| Started session plus new page | A client already has a session, then opens a new dashboard page | The new page inherits only that client's session, not another client's state |
| Parallel workers | Playwright distributes client tests across workers | At least two workers execute the parallel-client lane in the smoke report |
| Slow API while clients interact | `/api/overview` is slow during refresh/search/action | UI remains usable and stale responses are ignored |
| Partial logs failure | `/api/logs` fails while overview succeeds | Dashboard enters degraded mode without losing overview state |

## Not proven here

- Live Rust dashboard HTTP server behavior.
- Cross-machine browser behavior.
- Real MCP client process fan-out.
- Paid provider concurrency limits.
