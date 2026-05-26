# Full validation pass

Completed on Windows after the v0.6.9 runtime/classification update:

```bash
npm run check:ci
npm run check:rust
npm run build
npm run load:local -- --binary ./target/release/mcpace.exe --duration-ms 5000 --concurrency 64
npm run pack:npm:dry-run
npm publish --workspace @mcpace/cli --dry-run --json
npm run build:release-artifacts
```

Results:

- Node lint passed.
- 89 Node tests passed.
- `publint packages/npm/cli` passed.
- Release artifact dry-run passed.
- Rust format check passed.
- Clippy passed with `-D warnings`.
- 113 Rust tests passed.
- Release build passed.
- Local serve load test passed at 5 seconds / concurrency 64 with zero failed requests.
- npm pack dry-run and npm publish dry-run passed for `@mcpace/cli@0.6.9`.
- Source ZIP generation passed with 296 entries and no missing/extra/outside-root paths.

Real npm publication was not performed because this machine is not authenticated to npm (`npm whoami` returned 401).
