# Project Registry v1

## Current supported behavior

The Rust CLI currently supports **read-only inspection** of the project registry.

```bash
./target/release/mcpace projects list --json
```

The registry is read from:

- `MCPACE_STATE_ROOT/data/runtime/project-registry.json` when `MCPACE_STATE_ROOT` is set;
- otherwise `<repo-root>/data/runtime/project-registry.json`.

Project scanning and mutation are not implemented yet in the Rust-only repo.
