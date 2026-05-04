# Automation pass — 2026-04-19

## What was found and closed

- the npm launcher declared **Node 22+** support but did not fail fast on older Node runtimes;
- the repo had no canonical builder for a clean project source archive;
- `verify` was still a large single Rust file instead of a thin command-family root;
- CI checkout action version drift is now tracked by `reports/toolchain-support.json` and contract tests;
- archive shape, root naming, and junk-file exclusions were not machine-checked.

## What changed

- added `packages/npm/cli/lib/runtime.js` and a launcher guard so `@mcpace/cli` exits clearly on unsupported Node versions;
- added `scripts/archive-release.mjs` as the canonical clean source archive builder;
- added repo tests for archive shape and npm runtime policy;
- split `src/verify.rs` into `src/verify/{args,model,render}.rs` and kept `src/verify.rs` thin;
- updated CI to `actions/checkout@v6` and aligned stack metadata/docs;
- bumped repo version to `0.2.7` and kept manifests/reports aligned.

## Checks run in this container

```bash
npm test
npm run pack:npm:dry-run
npm run archive:release
node packages/npm/cli/bin/mcpace.js version
```

## Notes

- `node packages/npm/cli/bin/mcpace.js version` now fails intentionally on Node 18 with a clear support-floor message;
- `cargo test` and `cargo build --release` still require a host with Rust installed.
