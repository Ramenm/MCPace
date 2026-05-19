# Start here

MCPace is not ready to claim runtime beta until one real client can call one real upstream tool through MCPace.

Use this order:


Local-first proof path, without relying on paid GitHub features:

```bash
npm run verify:tooling
npm run verify:local-prepublish:quick
npm run verify:local-prepublish
```

Interpretation:

- `verify:tooling` checks whether the machine has the required local toolchain and recommended security/release tools.
- `verify:local-prepublish:quick` is the fast offline hygiene lane for active development.
- `verify:local-prepublish` is the full release-candidate gate. If it reports `blocked`, do not publish yet.

Read `docs/offline-quality-and-publish-gates.md` and `docs/tooling-stack.md` for the exact policy.

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
mcpace server install npm:@modelcontextprotocol/server-filesystem --as filesystem --path . --dry-run
mcpace server install npm:@modelcontextprotocol/server-filesystem --as filesystem --path .
mcpace server test filesystem --refresh --json
mcpace serve
mcpace client export cursor-local --json
```

Do not claim published binary install readiness until a native binary or platform package is staged and verified.
