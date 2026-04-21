# MCPace

`MCPace` is a Rust-first local MCP hub project.

The repository no longer ships PowerShell entrypoints. The active contract is a
native Rust CLI named `mcpace`, plus a thin npm launcher surface for users who
prefer npm-based installation.

This repo is intentionally honest about its state:

- implemented today: `version`, `doctor`, `init`, `hub up/down/repair/status/logs`, `stdio-shim --json` (bootstrap-only), `profile show`, `projects list`, `candidates`, `client list`, `client plan`, `client export` (preview-only), `lab list`, `lab matrix`, `lab coverage`, `lab gaps`, `lab report`, `lab show`, `server list`, `server capabilities`, `server candidates`, `verify doctor`, `verify readiness`, `repair`;
- the client catalog is now surface-aware: local, cloud, API connector, and generic surfaces are tracked separately;
- the repo now includes a local file-backed hub lifecycle surface for bootstrap, state, health, logs, corruption repair, and bounded log retention;
- planned next: `client install`, real config-writing `client export`, `release`, stdio ingress, HTTP ingress, and lease/scheduling enforcement;
- stack policy is now explicit and machine-readable: Node 22/24 LTS contributor lanes, default local Node 24 via `.nvmrc` / `.node-version`, npm 10+, and a pinned Rust 1.95.0 toolchain are tracked in `docs/toolchain-policy.md` plus `reports/toolchain-support.json`;
- **not** reconfirmed in this pass: live Docker/runtime behavior, or multi-host parity on Windows/macOS/Linux.

## Current direction

MCPace is moving toward a **single local MCP hub for many clients** with one
public binary and one user-facing command taxonomy.

Today the repo contains:

- Rust source under `src/`;
- npm launcher packaging under `packages/npm/cli`;
- schema, examples, docs, reports, and repo-contract tests;
- **no** active PowerShell runtime layer.

Unsupported commands are reported as **not implemented yet in the Rust-only
repo** instead of silently bridging to deleted scripts.

## Native commands available now

These commands are implemented directly in Rust source:

```bash
mcpace version
mcpace doctor
mcpace init --json
mcpace hub status --json
mcpace hub repair --json
mcpace hub logs --json --tail 20
mcpace stdio-shim --json --client-id codex --session-id demo-1 --project-root /work/project-a
mcpace repair --json
mcpace profile show --json
mcpace projects list --json
mcpace candidates --json
mcpace client list --json
mcpace client plan --json --client-id codex --session-id demo-1 --project-root /work/project-a
mcpace client export codex --json
mcpace lab matrix --json
mcpace lab report
mcpace server list --json
mcpace server capabilities --json --name browser
mcpace server candidates --json
mcpace verify doctor
mcpace verify readiness
```

The packaged npm launcher now fails fast on unsupported Node versions with a
clear message instead of trying to limp along below the declared Node 22+ floor.

To build a clean source archive with one meaningful root directory and no
`node_modules`, `.git`, caches, or build junk, run:

```bash
npm run archive:release
```

To regenerate the latest machine-readable verification artifact from executed
source/release checks in this environment, run:

```bash
npm run prove:report
```

That writes `reports/verification-latest.json` without pretending that missing
Rust/runtime proof has already passed.

`doctor/profile/projects/candidates/client-plan/lab/server/verify` now have native
Rust read paths, `init` seeds the runtime layout, `hub` provides a local lifecycle/status/log/repair surface, `client list` exposes the verified/generic client target catalog with surface-aware local/cloud/API distinctions,
and `lab` turns runtime fixtures plus capability inventory into an explicit backlog.

Compatibility aliases currently kept for a smaller migration gap:

- `project` -> `projects`
- `servers` -> `server list`
- `capabilities` -> `server capabilities`
- `check` / `probe` -> `verify doctor`
- `status` / `readiness` -> `verify readiness`

## Why `client plan` exists already

The future product promise is **one entry point for many clients**.
That only works if the hub owns session routing and upstream server arbitration
instead of letting each client guess for itself.

`client plan` is the first native control-plane slice for that promise:

- resolve client/session/project identity from explicit flags, env, or metadata;
- show the single-entry-point contract for future client installers/exporters;
- compute server isolation and request-serialization strategy from server policy;
- warn when project-local or single-session servers would be unsafe to share.

## Why `lab` exists already

It is too easy for a project like this to blur three very different claims:

- what the current code can already inspect or plan;
- what a future live hub should do;
- what still has no proof at all.

`lab` keeps those separate by reading:

- production-like runtime scenarios in `eval/fixtures/runtime/`;
- a capability inventory in `eval/runtime-capabilities.json`.

That gives you concrete answers to:

- which scenarios are **covered now**;
- which are only **partially covered**;
- which are still **blocked** by missing runtime or adapter work;
- which next steps close the biggest number of gaps.

For prompt / agent work, the repo now also carries grounded seed evals plus a
scenario map, scoring rubric, and regression plan under `eval/`. Those files are
meant to catch unsupported certainty, fake ETA precision, and vanity-benchmark
drift before they reach the user-facing docs or workflow.

## Grouped command surface

The target public surface remains grouped and smaller than the legacy script set:

```bash
mcpace init
mcpace hub up
mcpace hub repair
mcpace hub status
mcpace client install codex
mcpace client export codex
mcpace server list
mcpace profile show --json
mcpace projects list --json
mcpace verify doctor
mcpace verify readiness
mcpace repair
mcpace release
```

At this stage, `init`, `hub`, bootstrap-only `stdio-shim --json`, top-level
`repair`, and `client export` preview are implemented in source. Live
stdio forwarding, `client install`, config-writing `client export`, and
`release` still fail clearly as **planned but not implemented yet**.

## Toolchain lanes

See `docs/toolchain-policy.md` for the support policy. In short:

- contributors and CI should use Node 22 LTS or Node 24 LTS;
- the default local development line is Node 24 via `.nvmrc` and `.node-version`;
- the repo expects npm 10+;
- the Rust toolchain is pinned in `rust-toolchain.toml`;
- runtime proof still requires real supported hosts.

## Install and verification surfaces

The long-term install lanes for the same Rust binary are:

- GitHub Release platform archives;
- npm launcher package `@mcpace/cli`;
- later optional package-manager surfaces such as Homebrew or WinGet.

npm is a distribution surface, not a second implementation core.

## Spec baseline

The current checked MCP spec baseline is **2025-11-25**.

First-wave obligations:

- `stdio` transport;
- `Streamable HTTP` transport;
- HTTP `Origin` validation and localhost binding for local-only HTTP lanes;
- environment-sourced credentials for `stdio` lanes;
- stateful session routing across initialization, operation, and shutdown;
- cancellation/progress support awareness for long-running requests;
- no dependence on experimental tasks for the first correctness slice.

## Test and proof model

Treat proof layers separately:

1. **source proof** — manifests, schema/examples, repo-contract checks, docs/tests consistency;
2. **build proof** — `cargo build --release`, `cargo test`, `npm pack --workspace @mcpace/cli --dry-run`;
3. **runtime proof** — real host runs with Docker and supported transports;
4. **release proof** — repeatable artifacts and publish flow.

Passing one layer does not imply the others.

## Quick checks available in this repo

Useful verification commands in a toolchain-equipped environment:

```bash
cargo test
cargo build --release
npm test
npm run pack:npm:dry-run
```

Useful grouped checks after a successful Rust build:

```bash
./target/release/mcpace init --json
./target/release/mcpace hub status --json
./target/release/mcpace client list --json
mcpace client plan --json --client-id codex --session-id demo-1 --project-root /work/project-a
./target/release/mcpace lab report
./target/release/mcpace server list --json
./target/release/mcpace verify doctor
./target/release/mcpace verify readiness
```

## Project control docs

- `TODO.md` — prioritized backlog with status, dependencies, DoD, risks, and ETA ranges
- `STATE.md` — current verified status, progress view, assumptions, and next steps
- `DECISIONS.md` — active project decisions, alternatives, consequences, and review triggers

## Repository layout

- `src/` — Rust CLI and read-path implementation; `client`, `hub`, `lab`, and `server` now use thin module roots with focused submodules
- `packages/npm/cli` — thin npm launcher for the Rust binary
- `schemas/` — config schema
- `examples/` — example hub configs
- `docs/` — active design/runtime/test/release documentation
- `eval/` — runtime lab fixtures plus prompt/agent eval governance files
- `reports/` — coverage, verification, and release-summary artifacts
- `tests/` — Rust tests and Node repo-contract tests
- root project-control docs — `TODO.md`, `STATE.md`, `DECISIONS.md`

## Honesty rules

- Do not claim PowerShell support: the PowerShell layer was removed from this repo.
- Do not claim multi-host runtime parity from Node/source proof alone.
- Do not claim Docker or cross-host runtime proof from local build/test proof alone.
- Do not claim public release readiness until build + runtime + release proof exist.
