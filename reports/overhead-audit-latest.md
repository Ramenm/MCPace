# Overhead audit

- Status: pass
- Generated: 2026-05-19T12:36:05.699Z
- Project: mcpace 0.6.5
- CLI source bytes: 14128
- Vendored binary bytes: 3303408
- Launcher overhead: 49.08ms median delta
- Explicit binary launcher overhead: 50.96ms median delta
- In-process binary resolution p95: 153.4µs

## Checks

| Check | OK | Evidence |
|---|---:|---|
| root-workspace-has-no-runtime-or-dev-dependency-bloat | yes | 0 dependencies/devDependencies in root package.json |
| npm-cli-has-no-runtime-dependencies | yes | 0 dependencies in packages/npm/cli/package.json |
| optional-platform-dependencies-only | yes | @mcpace/cli-darwin-arm64, @mcpace/cli-darwin-x64, @mcpace/cli-linux-arm64-gnu, @mcpace/cli-linux-x64-gnu, @mcpace/cli-win32-arm64-msvc, @mcpace/cli-win32-x64-msvc |
| playwright-is-test-only-temp-install | yes | Playwright is not a runtime dependency and release manifest excludes node_modules |
| dashboard-source-footprint-under-100kb | yes | 38896 bytes |
| npm-launcher-source-footprint-under-20kb | yes | 14128 bytes |
| launcher-overhead-measured-or-blocked-explicitly | yes | median delta 49.08ms |
| resolve-binary-in-process-overhead-under-5ms-p95 | yes | p95 153.4µs over 250 runs |
| launcher-overhead-not-severe-on-this-host | yes | launcher median 69.21ms, delta 49.08ms |
| bounded-top-k-helper-shared | yes | Large tool-scale simulations use a shared bounded top-k helper instead of per-match full candidate sorting. |
| overhead-classifier-shared-policy | yes | Overhead benchmark/stress share the same signal policy library as package survey/profiling. |
