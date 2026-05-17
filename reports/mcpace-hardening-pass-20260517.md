# MCPace hardening pass — 2026-05-17

## Scope

This pass focused on the unfinished problem area around stateful/contextful/problematic MCP servers, random registry/server classification, conservative defaults, tests, and a minimal Dashboard surface.

## Implemented

- Fixed release evidence drift from `0.6.2` to `0.6.5` in eval fixtures.
- Restored POSIX executable bit on the vendored Linux binary and hardened `scripts/archive-release.mjs` so staged release archives preserve executable mode for the CLI and vendored binaries.
- Added explicit policy and review metadata to starter MCP presets:
  - filesystem: project-local, isolated per project/root, discovery requires lease;
  - git: project-local, single-writer per repository/root, discovery requires lease;
  - playwright: shared-exclusive, single session, browser-profile host lock, discovery requires lease;
  - context7: network-docs, multi-reader candidate, still review-required.
- Updated Rust source loader/model logic to expose `discoveryRequiresLease` and infer safer generic source policies for known risky families while keeping unknown sources conservative.
- Added a metadata-only Registry Lab:
  - `scripts/registry-lab.mjs`
  - `eval/fixtures/registry-sample.json`
  - `reports/registry-lab-latest.json`
  - `reports/registry-lab-latest.md`
  - `docs/registry-lab-and-policy-review.md`
- Wired `npm run verify:registry-lab` into `verify:hardening`.
- Added node contract tests for registry lab, preset policy, generic-source inference, and the minimal Dashboard policy/activity UI.
- Extended Dashboard source with:
  - Activity panel using `hub lease list --json` data;
  - Policy Review panel that calls out unknown/default-conservative, stateful, host-lock, and dangerous command-runner shapes.

## Verification performed

- `node scripts/check-node-syntax.mjs --json` — pass.
- `npm run verify:registry-lab` — pass.
- `npm run verify:vendored-binary` — pass.
- `npm run test:repo` — pass, 55/55 node test files.

## Known limitation

This environment does not have `cargo`, `rustc`, or `rustfmt`, so the Rust source changes were not compiled here. The bundled binary remains the original `0.6.5` binary and does not include the new Rust source/UI behavior until rebuilt with a Rust toolchain.

## Next release gate

Before claiming full production support, rebuild the binary from this source, run Rust tests on Linux/macOS/Windows, then run a live end-to-end proof:

`client -> MCPace /mcp -> upstream stdio server -> tool result`

and a conflict proof:

`client A + client B -> same stateful server -> lease/queue/block behavior`
