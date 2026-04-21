# @mcpace/cli

Thin npm launcher for the MCPace native Rust binary.

This package is not a second runtime core.
It resolves and launches an already available `mcpace` binary.

During local development, you can point it at a binary with:

- `MCPACE_BINARY_PATH`
- or `MCPACE_DEV_BINARY`


Supported Node floor: **22+**. The source workspace itself is maintained against Node 22 LTS and Node 24 LTS lanes.
