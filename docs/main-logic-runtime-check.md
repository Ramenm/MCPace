# Main logic runtime check

This document records the end-to-end invariants for MCPace's primary runtime path. It is intentionally higher level than an individual hardening note: the goal is to make the common user path easy to verify as one connected system.

## Primary path

1. The npm `mcpace` launcher resolves exactly one trusted native binary and delegates arguments without shell interpretation.
2. The native `app::run` dispatcher maps public commands and aliases to their subsystem entry points. `mcpace up` is the public setup alias and does not install a default upstream server unless the user supplies a server spec.
3. Setup creates or repairs the MCPace root, imports existing home MCP server definitions into the MCP settings namespace, starts the local endpoint, and wires supported clients to the stable Streamable HTTP endpoint.
4. MCP server mutations use the global MCP settings namespace lock plus deterministic per-file locks, so add/import/remove/enable/disable cannot silently shadow one another across source files.
5. Client install, server policy updates, home import, and project registry updates are read-modify-write flows and must hold their own exclusive locks before reading the current file.
6. Serve startup is single-writer guarded, dashboard/tool-list warmup is opt-in, and Streamable HTTP requests cross Host/Origin/session/header validation before they reach MCP routing.
7. Release publication must build and validate the platform-native packages before publishing the JavaScript launcher package.

## Why these checks exist

The main failure mode is not only a crash. The bigger risk is a workflow that appears to succeed while losing an update, selecting the wrong server record, warming up and starting user-supplied MCP commands unexpectedly, or publishing a launcher before the native target packages exist. The tests in `tests/node/main-logic-runtime.test.mjs` lock these invariants to the code layout so future refactors cannot accidentally split the runtime model again.
