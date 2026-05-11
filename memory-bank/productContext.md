# Product context

## Confirmed product contract

MCPace is positioned as a local-first MCP control plane and onboarding layer. The strongest honest current promise is one local MCPace endpoint plus generic MCP brokering/diagnostics for user-supplied upstream MCP servers.

## First ICP

Advanced integrator / solo power user who works across two or three local MCP clients, suffers from hand-maintained config drift, and is willing to run one local process for clearer routing and diagnostics.

## Activation levels

### Proven-today activation from repo docs

1. `mcpace client install <surface>` or `mcpace client export <surface>` succeeds.
2. `mcpace serve --port 39022` exposes `http://127.0.0.1:39022/mcp`.
3. A chosen client completes at least `initialize -> tools/list` against the localhost endpoint.

### Beta-quality activation

1. A real client reaches MCPace.
2. MCPace resolves session/project ownership correctly.
3. At least one upstream tool call succeeds without stale-result or ownership confusion.

## Product risk

The repo has strong proof/reporting infrastructure, but user-facing truth depends on not overstating runtime support. The product should be judged by the real local-client-to-upstream loop, not by docs volume or archive automation alone.


## v0.5.9 first-use simplification

MCPace now provides explicit useful-MCP install recipes as editable preset data. This improves first-run usability without changing product truth: no upstream server is enabled by default, and the user still runs an explicit `server install` or `server starter` command before an upstream is configured.
