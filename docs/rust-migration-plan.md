# Rust Migration Plan

## Goal

Move from a partially ported legacy-shaped repo to a truthful Rust-only repo with a smaller public CLI.

## What already happened

- PowerShell files were removed from the repository.
- repo-contract tests now guard against their return.
- active docs describe only the Rust-first path.
- grouped `server` and `verify` read-path commands now exist natively.
- `client plan` now exists in source as the first control-plane slice for future client routing and server arbitration.

## What is still not proven in this environment

- Rust build/test proof in the current container
- Docker runtime proof
- multi-host parity

## Next implementation order

1. reconfirm Rust build/test on a host with `cargo` and `rustc`;
2. move to a stronger grouped parser (`clap`) without regressing current commands;
3. add typed v2 config loading aligned with the schema;
4. add live arbitration/lease engine behind the current `client plan` model;
5. add hub lifecycle and state/log ownership;
6. add client install/export;
7. add release automation after cross-host build/runtime proof.

## Rule

Do not reintroduce a compatibility bridge to deleted scripts.
