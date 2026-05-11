# MCPace optimization pass

Generated: `2026-05-03T08:17:10Z`

## Size

| metric | before | after | delta |
|---|---:|---:|---:|
| project files | 459 / 4.0 MB | 428 / 2.7 MB | 1.3 MB saved (32.99%) |
| reports dir | 54 / 1.4 MB | 23 / 94.1 KB | 1.3 MB saved (93.58%) |
| optimized release zip | 898.3 KB original uploaded zip | 823.1 KB | 75.2 KB smaller (8.37%) |

## Main changes

- **Node test runner** — scripts/run-node-test-files.mjs now supports --jobs <n|auto>, defaults to bounded auto-parallel jobs, and preserves a serial lane for mutation-sensitive proof/npm-pack contract files.
- **Node syntax lint** — scripts/check-node-syntax.mjs now auto-discovers JS/MJS files and checks them with bounded auto-parallel workers; MCPACE_NODE_SYNTAX_JOBS or --jobs 1 can cap/serialize it.
- **Runtime resources** — src/resources.rs now exposes bounded environment override knobs for HTTP connection limits, body/time budgets, dashboard cache windows, upstream workers, and session-pool sizing.
- **Stack metadata** — packageManager and supporting docs/workflows moved to npm@11.13.0; Node 22/24 and Rust 1.95.0 remain the project policy.
- **Release weight** — release-manifest.json now includes a curated report set instead of the entire historical reports directory; obsolete bulky smoke/sweep reports were pruned from the optimized tree.
- **Docs/contracts** — README/docs/source-quality/test-strategy/verification docs now describe the new resource knobs and bounded auto-parallel lanes.

## Validation

| command | status | evidence |
|---|---|---|
| `npm run lint:node` | pass | 80/80 JS/MJS files checked; jobs=6 |
| `npm run test:npm -- --json` | pass | 3/3 npm CLI test files passed; parallel jobs=3 |
| `node --test selected/chunked tests/node contract files` | pass | 79 subtests passed across archive, boot, product-practice, runtime-performance, source-quality, release, evidence, packaging, proof, and npm-pack contract files |
| `cargo fmt --all -- --check` | pass | rustfmt check completed with exit code 0 |
| `cargo check --all-targets --locked` | blocked | sandbox DNS could not resolve index.crates.io while fetching auto-launch dependency |
| `cargo check --all-targets --locked --offline` | blocked | offline cache lacks getrandom, so dependency resolution cannot complete here |
| `npm run test:repo -- --json` | not-completed-in-sandbox | full monolithic repo runner exceeded the container execution window; the same test files were run in smaller chunks/selected lanes successfully |

## Release archive

- Path: `/mnt/data/mcpace_optimized_dist/mcpace-v0.5.9-030526-081300.zip`
- Size: `823.1 KB`
- SHA-256: reported in the final delivery message, outside the archive, to avoid self-referential hash drift.
