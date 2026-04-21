# Runtime Profiles

## Current supported behavior

The Rust CLI currently supports **read-only inspection** of runtime profiles.

```bash
./target/release/mcpace profile show --json
```

The active profile is resolved in this order:

1. `MCPACE_RUNTIME_PROFILE`
2. `manager.settings.json` as a legacy compatibility input
3. config default from `mcpace.config.json`
4. safe fallback

Mutation flows such as changing or applying a profile are not implemented yet in the Rust-only repo.
