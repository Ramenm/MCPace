# Overhead audit

- Status: pass
- Generated: 2026-05-16T13:58:09.919Z
- Project: mcpace 0.6.4
- CLI source bytes: 13842
- Vendored binary bytes: 3303408
- Launcher overhead: 131.92ms median delta

## Checks

| Check | OK | Evidence |
|---|---:|---|
| root-workspace-has-no-runtime-or-dev-dependency-bloat | yes | 0 dependencies/devDependencies in root package.json |
| npm-cli-has-no-runtime-dependencies | yes | 0 dependencies in packages/npm/cli/package.json |
| optional-platform-dependencies-only | yes | @mcpace/cli-darwin-arm64, @mcpace/cli-darwin-x64, @mcpace/cli-linux-arm64-gnu, @mcpace/cli-linux-x64-gnu, @mcpace/cli-win32-arm64-msvc, @mcpace/cli-win32-x64-msvc |
| playwright-is-test-only-temp-install | yes | Playwright is not a runtime dependency and release manifest excludes node_modules |
| dashboard-source-footprint-under-100kb | yes | 32365 bytes |
| npm-launcher-source-footprint-under-20kb | yes | 13842 bytes |
| launcher-overhead-measured-or-blocked-explicitly | yes | median delta 131.92ms |
| launcher-overhead-not-severe-on-this-host | yes | launcher median 153.32ms, delta 131.92ms |
