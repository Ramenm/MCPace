# Recovery Runbook

## What to do first

1. Run source checks.
2. Confirm manifests, schema, and docs are aligned.
3. Only after that, move to host build/runtime proof.

## Source checks

```bash
npm test
npm run pack:npm:dry-run
```

## Build checks on a Rust host

```bash
cargo build --release
cargo test
```

## Important rule

Do not attempt to recover by reviving deleted PowerShell scripts. If documentation or tooling still depends on them, fix the stale contract instead.
