# Rewrite Cutover Plan

## Cutover status

The repository cut over from "Rust plus PowerShell bridge" to a **Rust-only source contract**.

That does **not** mean runtime parity is complete.
It means the repo no longer pretends deleted PowerShell commands still exist.

## Rules after cutover

- no new `.ps1` files
- no `pwsh` instructions in active docs
- no bridge modules that shell out to removed scripts
- planned commands must say "not implemented yet" explicitly

## Current safe slices already completed

1. stabilize read-only commands
2. add grouped verification commands
3. add grouped server inspection commands
4. prove Linux source/build locally without external Cargo dependencies

## Next safe implementation slices

1. move to `clap` command taxonomy
2. add typed config loading aligned with schema
3. add hub lifecycle
4. add client install/export
5. add release automation
