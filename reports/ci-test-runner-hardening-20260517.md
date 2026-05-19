# CI test-runner hardening — 2026-05-17

Status: `pass`

## Issue fixed

Full Node repo tests stalled around the product-practice isolated lane. The root risk was sync child-process execution in a test runner that also runs tests which spawn nested subprocesses.

## Change

Mutation-sensitive tests are still isolated into one-file lanes, but every file now uses the async detached process-group runner with explicit timeout and process-tree termination. Release publication now also has a strict fail-closed command: `npm run verify:publish-decision:release`.

## Evidence

- Node test report: `pass`.
- Selected/completed: `57/57`.
- Passed: `57`.
- Batches: `26/26`.
- Publish decision: `blocked`; native publication allowed: `False`.
- Runner contract asserts `spawnSync` is absent from `scripts/run-node-test-files.mjs` and detached process groups plus `terminateChild(child)` remain wired.

## Remaining blockers

- Rust rebuild proof still requires cargo/rustc on a build host.
- Docker/destructive sandbox lanes are still not proven in this environment.
- `verify:local:source` still depends on Cargo-source steps and is not a completed source profile in this container.
