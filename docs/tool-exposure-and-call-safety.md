# Tool exposure and upstream call safety

This is the operating contract for MCPace tool visibility and upstream tool execution.

## Problem

MCP clients decide what to call from the tools visible in `tools/list` plus the natural-language descriptions returned by discovery tools. With many upstream servers, accidental exposure is risky: a stale cached tool, a hallucinated name, a malicious description, or a broad native projection can make a client try a tool that should have stayed behind brokered routing.

## Rules

1. **Broker-first default**: `MCPACE_TOOL_EXPOSURE=broker` keeps upstream tools out of startup `tools/list`.
2. **Safe projection default**: `MCPACE_PROJECTED_TOOL_SAFETY=safe` means only tools that look read-only through trusted annotations or conservative names may become native projected tools.
3. **Known-tool call guard**: `upstream_call`, `upstream_batch`, and projected `u_*` calls must verify that the upstream server currently advertises the target tool in `tools/list` before forwarding the call.
4. **Explicit dynamic-tool escape hatch**: pass `allowUnknownTool=true` only when a trusted upstream intentionally supports hidden/dynamic tools. Operators may also set `MCPACE_ALLOW_UNKNOWN_UPSTREAM_TOOLS=true` for legacy environments, but that weakens the guard globally.
5. **Metadata-injection is risky even before execution**: suspicious tool titles/descriptions such as prompt-override, secret-exfiltration, or credential-stealing language are classified as `metadata-injection` advisory risk and should not be natively projected without policy review.
6. **Policy remains separate**: known-tool validation only proves that the upstream advertises the name. Mutating, desktop, interaction, system-control, metadata-injection, or open-world tools still require declarative `toolPolicies` or explicit risk-class opt-ins when policies say so.
7. **Discovery is not execution**: stale cache can support search/catalog display, but a live call is still a live operation and must pass current tool-name and policy gates.

## Practical flow

For a user task, clients should prefer:

```text
adapter_profile       # understand current surface/mode
upstream_search       # find a relevant tool without loading everything
adapter_route         # group same-server/stateful calls when needed
upstream_call/batch   # execute only advertised, policy-allowed tools
```

For strict clients or suspicious upstreams:

```bash
MCPACE_TOOL_EXPOSURE=broker
MCPACE_PROJECTED_TOOL_SAFETY=safe
MCPACE_MANAGEMENT_SURFACE=minimal
```

For native projected tools in a small trusted catalog:

```bash
MCPACE_TOOL_EXPOSURE=auto
MCPACE_PROJECTED_TOOL_SAFETY=safe
MCPACE_TOOL_BUDGET=64
MCPACE_TOOL_TOKEN_BUDGET=24000
```

## Do not

- Do not set `MCPACE_PROJECTED_TOOL_SAFETY=all` for untrusted servers.
- Do not use all-server `upstream_catalog` as a long-lived routing cache.
- Do not call a tool by name just because a model guessed it. Use `upstream_search` or `upstream_tools(server)` first.
- Do not use `allowUnknownTool=true` as a workaround for stale caches; refresh discovery instead.
