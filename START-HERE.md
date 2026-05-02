# Start here

MCPace is not ready to claim runtime beta until one real client can call one real upstream tool through MCPace.

Use this order:

```bash
npm run lint:npm
npm run verify:boot
npm run verify:product-practice
npm run verify:runtime-trace
```

Interpretation:

- `lint:npm` checks every JS/MJS source by discovery. Do not add hardcoded file lists to `package.json`.
- `verify:boot` answers whether the source tree, package shape, toolchain, and install mode are coherent.
- `verify:product-practice` prevents a common bad practice: adding more features and reports while publish/install proof remains missing.
- `verify:runtime-trace` records the local broker loop proof when a compiled binary exists: `/mcp` initialize, tools/list, and `upstream_call` into the tiny stdio fixture.

First user path:

```bash
mcpace connect
mcpace server presets
mcpace server starter --path . --dry-run
mcpace server starter --path .
mcpace server test filesystem --refresh --json
mcpace serve
mcpace client export cursor-local --json
```

Do not claim published binary install readiness until a native binary or platform package is staged and verified.
