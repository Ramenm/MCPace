# @mcpace/cli

Thin npm launcher for the MCPace native Rust binary.

This package is not a second runtime core.
It resolves and launches an already available `mcpace` binary.

The current public promise for MCPace is **one local MCPace endpoint, simpler install on selected local clients, and honest diagnostics for what is configured versus actually usable.**

This launcher stays inside that promise: it helps users reach the native binary, but it does **not** turn the package into a second runtime core or a present-tense universal-runtime claim.

Resolution order:

1. explicit `MCPACE_BINARY_PATH` / `MCPACE_DEV_BINARY`
2. local workspace binaries under `target/` or `dist/`
3. vendored package binaries under `packages/npm/cli/vendor/<target>/` or `vendor/<target>/`
4. optional platform packages such as `@mcpace/cli-linux-x64-gnu`

If you already built the current host binary from source, you can stage it into the
vendored layout with:

```bash
node scripts/stage-vendored-binary.mjs --json
node scripts/verify-vendored-binary.mjs --json
```

Supported Node floor: **22+**. The source workspace itself is maintained against Node 22 LTS and Node 24 LTS lanes.

The machine-generated `reports/verification-latest.json` snapshot now records whether the current target is self-contained, source-build-only, or blocked without a vendored binary or Rust toolchain.
