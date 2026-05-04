# Runtime Beta Roadmap

Runtime beta means MCPace is no longer only source-ready. It means a real user can connect a real local MCP client, see tools, call an upstream tool through MCPace, and understand failures without reading source code.

## Durable HTTP sessions

HTTP session lifecycle is the first beta gate. The source now contains an in-process store that creates a session on `initialize`, stores client metadata and protocol version, reuses and touches known sessions, rejects unknown or expired sessions, and closes sessions on `DELETE /mcp`. The remaining beta work is to prove this through fresh host/runtime traces and decide whether cross-process persistence is needed for public/relay modes.

Acceptance criteria:

- `initialize` creates a bounded visible-ASCII session id.
- later stateful requests require a known session unless compatibility mode is explicitly enabled;
- expired, closed, unknown, and protocol-mismatched sessions are tested;
- session state is visible in diagnostics without leaking credentials.

## HTTP upstream fan-out

HTTP upstream fan-out turns configured Streamable HTTP upstreams from inventory-only into callable upstreams.

Acceptance criteria:

- common connector interface for stdio and HTTP;
- initialize, tools/list, and tools/call work against a tiny HTTP MCP fixture;
- URL validation rejects unsafe schemes and suspicious targets;
- auth headers are isolated per upstream;
- timeouts, cancellation, and bounded diagnostics are tested.

## Real-client traces

Real-client traces prove that the product works outside unit tests.

Acceptance criteria:

- at least one local GUI/editor client reaches `http://127.0.0.1:39022/mcp`;
- the client completes `initialize -> tools/list -> tools/call` through MCPace;
- the trace names OS, architecture, client version, MCPace version, upstream server, and exact result;
- generated reports are fresh for the current target.

## Release proof

Runtime beta needs install proof too. A user should not need a Rust toolchain just to try the product.

Acceptance criteria:

- at least one native target has a staged and verified vendored binary;
- platform package manifests match `release-targets.json`;
- checksums and dry-run npm tarballs are generated;
- the README states which install path is actually proven.
