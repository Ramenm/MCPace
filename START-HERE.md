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
- `verify:product-practice` prevents a common bad practice: adding more features and reports while the core broker proof remains missing.
- `verify:runtime-trace` is the required next runtime proof lane. It stays blocked until a compiled binary and real `/mcp` trace exist.

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
