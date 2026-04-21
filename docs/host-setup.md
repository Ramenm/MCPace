# Host Setup

## Current host baseline for real proof

Required for build proof:

- Rust 1.95.0 (from `rust-toolchain.toml`, includes `cargo` and `rustc`)
- Node.js 22 LTS or 24 LTS
- npm 10+ (the repo pins `npm@11.12.1` as the default package manager hint)

Preferred local bootstrap:

- `nvm use` should pick up **`.nvmrc`**
- other version managers can follow **`.node-version`**
- the default local line is **Node.js 24**, while **Node.js 22** remains a supported CI lane
- `reports/toolchain-support.json` is the machine-readable stack reference

Required for runtime proof:

- Docker Engine or Docker Desktop

## Confirmed in the current container

The following commands were actually run successfully in the current container:

```bash
npm test
npm run pack:npm:dry-run
```

## Not currently available in this container

- `cargo`
- `rustc`
- Docker daemon/runtime proof

So Rust build proof and runtime proof still require a real host or a container with
those tools installed.

## Still required on real supported hosts

Runtime and cross-host proof still need real Linux, Windows, and macOS runs:

```bash
cargo test
cargo build --release
./target/release/mcpace client plan --json --client-id codex --session-id demo-1 --project-root /work/project-a
./target/release/mcpace verify doctor
./target/release/mcpace verify readiness
```

For a faster launcher-only proof before the full matrix:

- on **Windows**, run
  `cargo test --test hub_runtime hub_up_releases_captured_stdio_for_background_launcher -- --exact`
- for **Ubuntu** from any Docker-capable host, run
  `node scripts/verify-ubuntu-docker-fast.mjs --json`
- for **Ubuntu E2E** from any Docker-capable host, run
  `node scripts/verify-ubuntu-docker-e2e.mjs --json`
- for **Ubuntu full-work** from any Docker-capable host, run
  `node scripts/verify-ubuntu-docker-full.mjs --json`
- for **macOS**, run the same targeted Rust test on a real `macos-latest` runner or
  real Apple hardware

## Important boundary

This repository no longer requires or ships PowerShell entrypoints.
If a workflow depends on deleted `.ps1` files, it is stale and must be updated
rather than worked around.

## Maintainer note

The active contributor stack is recorded in `docs/toolchain-policy.md`,
`reports/toolchain-support.json`, `.nvmrc`, `.node-version`, plus `engines`,
`packageManager`, and `devEngines` inside `package.json`.

For launcher and detach semantics, trust real runners over emulators. macOS process
behavior should be proven on real Apple-hosted runners or hardware, not on synthetic
virtualization layers that are hard to align with release behavior.

For Linux, prefer the constrained Docker lanes in this repo over ad-hoc host runs.
The default Ubuntu Docker verification scripts run with bounded CPU, memory, and pid
limits so performance and startup behavior stay closer to a realistic low-noise
operator envelope.

The constrained Ubuntu Docker E2E lane covers both recovery and normal lifecycle:
it proves `hub status`, `hub repair`, `hub up`, `hub logs`, and `hub down` in one
bounded-resource pass.

The constrained Ubuntu Docker full-work lane adds the repo-root release path on
top of that lifecycle proof. It builds the release binary inside a verify image
that includes Rust 1.95 and Node 24, then exercises `mcpace version`,
`mcpace doctor`, `mcpace client list`, `mcpace client plan`,
`mcpace server list`, `mcpace verify doctor`, `mcpace verify readiness`,
top-level `mcpace repair`, `hub up`, `hub logs`, and `hub down`.
