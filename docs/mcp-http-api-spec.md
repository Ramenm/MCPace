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

Loopback local mode does not require authentication by default because it is
intended for same-host native clients. Operators can require bearer-token
authentication by setting `MCPACE_HTTP_AUTH_TOKEN`, or by passing
`--auth-token-env <NAME>` and placing the token in that environment variable.
When a token is configured, every HTTP request must include
`Authorization: Bearer <token>` and unauthorized requests receive
`401 Unauthorized` with `WWW-Authenticate: Bearer realm="mcpace"`.

Non-loopback bind hosts such as `0.0.0.0` are rejected by default. They require
`--allow-nonlocal-bind` plus bearer-token authentication. The only unauthenticated
non-loopback mode is the deliberately named `--insecure-nonlocal-bind`, intended
for short-lived lab use only and not for public or shared networks.

## Common headers

### Request headers

- `Host`: required exactly once and must resolve to an exact loopback authority: `127.0.0.1`, `localhost`, or `[::1]`, with an optional numeric port. Missing, duplicate, host-suffix, userinfo, path-bearing, and non-loopback hosts are rejected.
- `Origin`: optional. Native clients normally omit it. When present, it must be an allowed loopback browser origin such as `http://127.0.0.1:<port>`, `http://localhost:<port>`, or `http://[::1]:<port>`. `null`, `file://`, userinfo, and host-suffix tricks are rejected.
- `Accept`: required for POST. Must include both `application/json` and
  `text/event-stream`.
- `Content-Type`: required to be `application/json` for POST bodies. A parameter such as `charset=utf-8` is allowed, but a missing or non-JSON content type is rejected.
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
- `Content-Length`: at most one header is accepted. Duplicate or unparsable
  values are rejected before request dispatch.
- `Transfer-Encoding`: not implemented by this local listener and rejected
  before request dispatch to avoid ambiguous request-boundary handling.

### Response headers

- `Content-Type: application/json; charset=utf-8` for JSON responses.
- `Cache-Control: no-store`.
- `Allow: POST` when GET SSE is not supported and the route returns `405`.
- `Mcp-Session-Id` on `initialize` responses. MCPace generates this server-side, stores the negotiated protocol and client metadata in a bounded in-process session store, and intentionally does not trust client-supplied session ids during initialize. Later stateful requests must echo the server-issued value.
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
  missing required POST `Accept` entries, missing/invalid `Mcp-Session-Id`
  after initialization, a protocol-version mismatch for the active session, or
  a `Mcp-Method` / `Mcp-Name` header that disagrees with the JSON-RPC request
  body. Header/body mismatches use JSON-RPC error code `-32001`
  (`HeaderMismatch`).
- `403 Forbidden`: invalid `Host` or `Origin`.
- `404 Not Found`: unknown, expired, or already-closed `Mcp-Session-Id` on a
  stateful request. Clients should initialize again before retrying stateful
  calls.
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

Terminates a Streamable HTTP session. The request must include a known
`Mcp-Session-Id` header. MCPace removes that session from the in-process store
and returns an empty response.

Status: `202 Accepted`

Errors:

- `400 Bad Request` for a missing or invalid `Mcp-Session-Id` header.
- `404 Not Found` for an unknown, expired, or already-closed session id.
