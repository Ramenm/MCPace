# MCPace v0.6.9 source-bundle summary

## Product direction

MCPace is a local MCP scheduler for concurrent AI agents. The product promise is not only one stable endpoint; it is safe runtime adaptation for fragile upstream MCP servers.

Core rule:

```text
server -> evidence -> runtimeType/stateClass/effectClass -> concurrencyPolicy
```

## Current source-bundle contract

The bundle is source-only and keeps one clean root directory. It includes code, configs, schemas, examples, docs, tests, evaluation fixtures, and this summary. It excludes `.git`, `node_modules`, caches, runtime logs/data/backups, vendored platform binaries, Rust `target`, and heavy build outputs.

## Documentation normalization

The docs are intentionally split by job:

| File | Job |
|---|---|
| `README.md` | Short landing page and first commands. |
| `docs/README.md` | Runbook and documentation map. |
| `docs/architecture.md` | Scheduler architecture, modes, and state classes. |
| `docs/configuration.md` | Config files, dynamic discovery, and policy options. |
| `docs/lab-harness.md` | Evidence corpus, random sweeps, and safe probe boundary. |
| `SECURITY.md` | Vulnerability reporting and security posture. |

Historical validation notes were condensed here instead of being repeated across user-facing docs.

## Runtime classification guardrails

MCPace keeps unknown stdio servers conservative. Name-only evidence is never enough to widen concurrency. A random package must remain plan-only, needs-safe-probe, blocked, or unknown-conservative until stronger metadata, trusted catalog data, or safe live MCP surface evidence exists.

Important hardening points:

- broad substring matching was replaced with token/boundary matching;
- GitHub/GitLab-style APIs are not confused with local `git` workers;
- short destructive tokens such as `rm` count only as standalone tool/command tokens;
- browser control, browser observation, and browser data are separated;
- dependency names and README install snippets are not trusted semantic evidence;
- single-writer and project-isolated servers keep `maxWorkers=1` and scale by partition, not by concurrent calls into one fragile worker.

## Lab evidence

The lab corpus ships normalized fixtures and ledgers under `eval/`.

Metadata and package analysis was performed without executing foreign MCP server code; not executing foreign MCP server code is the explicit safety boundary.

Recorded evidence includes:

- popular npm and PyPI package metadata;
- `npm pack` metadata/package artifact inspection for selected packages;
- `pip download --no-deps` metadata/package artifact inspection for selected PyPI packages;
- random held-out audit data for unfamiliar MCP packages;
- random 100-package and 500-package npm sweeps;
- second-pass review for every server in the 500-package sample;
- final auto-readiness ledger.

Downloaded `.tgz` and `.whl` files are not included in the repository or release bundle.

## Dynamic discovery / auto mode

User flow stays simple:

```bash
mcpace auto --dry-run
mcpace auto
mcpace lab probe --refresh --timeout-ms 30000
```

`mcpace auto` may refresh stale registry metadata, select approved/trusted candidates, write server fragments, and run safe `initialize` plus `tools/list` probes. Unknown public packages are not silently executed.

## Dashboard observability UI/backend pass

The bundled dashboard now has a tighter frontend/backend contract:

- the UI checks the live backend through `GET /api/overview`, `GET /api/logs`, `GET /api/resources`, and a dedicated safe `POST /api/actions/ping` instead of using a write-oriented autotune endpoint as a connectivity probe;
- dashboard fetches use bounded timeouts, abort stale refreshes, and show partial backend state when logs/resources fail independently of the core overview;
- action buttons record the actual action endpoint, duration, and result so the backend link card reflects the last write path, not only passive reads;
- `/api/resources` is surfaced as a first-class runtime check in the UI so operator-visible HTTP/session/pool counters match the backend state endpoint.

The bundled dashboard also keeps the stricter progressive-disclosure layout:

- the first screen shows only four essential answers: system state, attention count, servers, and load;
- warnings/blockers and the compact server list are the only primary operational panels;
- instance plan, policy review, capacity, telemetry coverage, client surfaces, audit entries, and raw logs live under `Deep diagnostics` by default;
- each server row stays compact but can reveal settings/routing details on demand;
- local-only view preferences cover auto-refresh, server sorting, attention-only filtering, row detail expansion, density, search, and enabled-only filtering;
- potentially disruptive actions (`Stop hub`, `Repair`) still require confirmation;
- the dashboard does not invent per-server CPU/RAM or request-latency percentiles. Those remain telemetry gaps until the runtime exposes process-level resource usage and request-duration histograms.

This keeps the UI useful for normal operation without turning the default view into a wall of metrics.


## Internal inventory / ownership pass

This bundle now includes a dependency-free static inventory:

- `npm run inventory` regenerates `reports/internal-inventory.md` and `reports/internal-inventory.json`;
- the inventory maps command groups, grouped subcommands, architecture slices, largest Rust files, duplicate function-name pressure, intentionally bounded/unfinished surfaces, and end-to-end runtime flows;
- the goal is to make future refactors additive and controlled: keep one owner for each responsibility and split large files along existing `args/model/render/runtime/tests` seams instead of adding parallel implementations.

Current inventory headline:

- 23 public command groups, all marked implemented in the catalog;
- 125 Rust files and 1500 parsed Rust functions;
- 22 Rust files at or above 700 lines;
- MCP tool surfaces are now connected in the inventory: stdio exposes 25 native tools, Streamable HTTP exposes 24, 22 are common, and the remaining delta is explicit (`runtime_acquire/renew/release` stay stdio-only; `hub_repair/runtime_diagnostics` stay HTTP-only);
- `tests/node/mcp-surface-connectivity.test.mjs` checks that HTTP annotations do not point at dead tools and that every declared stdio/HTTP tool has a runtime dispatch path;
- main split candidates: `src/server/loader.rs`, `src/setup.rs`, `src/adapter/discovery.rs`, `src/dashboard.rs`, `src/upstream/lease_runtime.rs`, `src/mcp_server.rs`, `src/serve.rs`, `src/hub/leases.rs`, `src/upstream.rs`, `src/server/discover.rs`;
- intentionally bounded surfaces still include bootstrap-only stdio shim forwarding, direct HTTPS upstream forwarding without a TLS adapter, project scanning, and profile mutation.

The second connectivity pass also exposed that several safe read-only management helpers were annotated for the Streamable HTTP surface but not actually declared/dispatched there. The HTTP MCP surface now has connected implementations for `runtime_leases`, `server_capabilities`, `client_plan`, and `client_export`, while explicit lease mutation remains stdio-only.

## Validation status for this cleanup pass

Run in this sandbox after cleanup:

- `npm run check` — 95/95 Node-side tests passed after adding MCP surface connectivity coverage
- `npm run inventory`
- `npm run lint:npm`
- `npm run release:dry-run`
- `npm run pack:npm:dry-run`
- npm package `files` whitelist points only at existing source paths in this bundle
- npm package bin shim `packages/npm/cli/bin/mcpace.js` exists, keeps executable bits, and forwards user arguments to the resolved native binary
- Markdown link/path audit for user-facing docs
- JSON/YAML/TOML parse audit for structured documentation and governance files
- GitHub issue-template label audit against `.github/labels.yml`
- Third-pass per-document/governance audit across README, docs, reports, package metadata, issue forms, workflows, and release manifest
- GitHub Actions artifact upload/download major audit and security workflow trigger reachability audit

Rust-host validation is still required on a machine with the pinned Rust toolchain:

- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`
- `cargo build --release`
- `npm run load:local -- --binary ./target/release/mcpace --duration-ms 5000 --concurrency 64`

Reason: this sandbox does not provide a Rust toolchain or prebuilt native binary. An attempted `npm run check:rust` stopped at the project Cargo preflight with a clear “cargo was not found” message.
