# MCP HTTP API specification

## Format

This project does not currently use OpenAPI, GraphQL schema, proto, or RAML for
the MCP endpoint. The effective API contract is MCP JSON-RPC over the local
HTTP `/mcp` route, implemented in `src/dashboard.rs` and documented here as a
developer-facing text spec.

## Base URL

Local unified serve endpoint default:

```text
http://127.0.0.1:39022/mcp
```

The advertised client URL is resolved from `MCPACE_PUBLIC_MCP_URL`, `mcpace.config.json` `serve.publicUrl`, `MCPACE_SERVE_HOST` / `MCPACE_SERVE_PORT` / `MCPACE_SERVE_PATH`, `mcpace.config.json` `serve.host` / `serve.port` / `serve.mcpPath`, then the default above. Unified serve accepts both the default `/mcp` path and the configured `serve.mcpPath`; `mcpace serve --host` and `mcpace serve --port` still override the bind address for that process.

## Authentication and authorization

Current local endpoint authentication is **НЕ ПОДТВЕРЖДЕНО** in this archive.
For local mode, the implemented hardening is origin validation, localhost-first
binding policy, and explicit MCP request validation. Do not treat this as a
remote authenticated API.

## Common headers

### Request headers

- `Origin`: optional. When present, it must be an allowed local origin such as
  `http://127.0.0.1:<port>`, `http://localhost:<port>`, `http://[::1]:<port>`,
  or `null`.
- `Accept`: required for POST. Must include both `application/json` and
  `text/event-stream`.
- `Content-Type`: expected to be `application/json` for POST bodies.
- `MCP-Protocol-Version`: optional in this implementation; unsupported values
  produce `400 Bad Request`.
- `Mcp-Method`: optional compatibility header in this implementation, but when
  present it must match the JSON-RPC `method` field. This guards against
  header/body request smuggling as MCP clients and proxies move toward the
  draft/SEP-2243 standard request-header contract.
- `Mcp-Name`: optional compatibility header in this implementation, but when
  present it must match `params.name` for `tools/call` and `prompts/get`, or
  `params.uri` for `resources/read`. A `Mcp-Name` header on methods without a
  name/URI source is rejected.

### Response headers

- `Content-Type: application/json; charset=utf-8` for JSON responses.
- `Cache-Control: no-store`.
- `Allow: POST` when GET SSE is not supported and the route returns `405`.
- `Mcp-Session-Id` on `initialize` responses. MCPace currently mints a compatible session id for clients to echo on later requests; durable server-side HTTP session storage is still a future hardening step.
- `MCP-Protocol-Version` on `initialize` responses.

## Operations

### POST `/mcp`

Send one JSON-RPC request, notification, or response.

#### Example initialize request

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "initialize",
  "params": {
    "protocolVersion": "2025-11-25",
    "capabilities": {},
    "clientInfo": {
      "name": "example-client",
      "version": "0.1.0"
    }
  }
}
```

#### Success response

Status: `200 OK`

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "protocolVersion": "2025-11-25",
    "capabilities": {},
    "serverInfo": {
      "name": "mcpace",
      "version": "0.5.5"
    },
    "instructions": "..."
  }
}
```

Response headers include `Mcp-Session-Id` and `MCP-Protocol-Version`.

#### Notification response

For accepted notifications such as `notifications/initialized`:

Status: `202 Accepted`

Empty body.

#### Error statuses

- `400 Bad Request`: invalid JSON-RPC body, unsupported protocol version,
  missing required POST `Accept` entries, or a `Mcp-Method` / `Mcp-Name`
  header that disagrees with the JSON-RPC request body. Header/body mismatches use JSON-RPC error code `-32001` (`HeaderMismatch`).
- `403 Forbidden`: invalid `Origin`.
- `413 Payload Too Large`: request body exceeds configured limit.

### GET `/mcp`

Used as a lightweight endpoint description unless the client asks for SSE.

#### Non-SSE response

Status: `200 OK`

```json
{
  "ok": true,
  "surface": "unified-serve-http",
  "message": "Use HTTP POST with a single JSON-RPC request body at this endpoint."
}
```

#### SSE request when streaming is unsupported

Request header:

```text
Accept: text/event-stream
```

Status: `405 Method Not Allowed`

Required response header:

```text
Allow: POST
```

### Unsupported routes

Status: `404 Not Found`

Plain text body: `Not found`.

## Breaking changes

The added `Accept` validation is a behavior tightening. Clients that previously
sent POST `/mcp` without both `application/json` and `text/event-stream` should
be updated before depending on this version.

`Mcp-Method` / `Mcp-Name` validation is currently mismatch-only compatibility
hardening: old clients that omit those headers still work, but clients or
intermediaries that send conflicting header/body values receive `400 Bad
Request`. A future strict draft/later-protocol mode may require those headers on all POST
requests.

## How to verify

Source-level checks:

```bash
npm run test:repo && npm run test:npm
```

Rust/runtime checks still needed on a Rust-enabled host:

```bash
cargo fmt --all -- --check
cargo test --all-targets --locked
npm run verify:rust-quality
```


### DELETE `/mcp`

Terminates a Streamable HTTP session from the client perspective. Current implementation accepts the request after Origin validation and returns an empty `202 Accepted`; durable session cleanup is not yet implemented.

Status: `202 Accepted`
