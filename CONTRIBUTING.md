# Contributing

## Working assumptions

- The active implementation core is Rust.
- npm exists only as a thin launcher/install surface.
- PowerShell is no longer part of the active repository contract.

## Supported contributor stack

- Node 22 LTS or Node 24 LTS
- npm 10+
- Rust 1.95.0 from `rust-toolchain.toml`

The preferred local default is Node 24 from `.nvmrc` / `.node-version`.
See `docs/toolchain-policy.md` and `reports/toolchain-support.json` for the
policy and upgrade rules.

## Minimum contributor workflow

Run the source-level checks first:

```bash
npm test
npm run pack:npm:dry-run
```

On a host with Rust toolchain available:

```bash
cargo build --release
cargo test
```

## Review rules

- Keep facts, assumptions, and proof layers separate.
- Do not reintroduce deleted PowerShell entrypoints.
- Keep docs, tests, manifests, local version files, CI, and reports aligned with the actual repo state.
