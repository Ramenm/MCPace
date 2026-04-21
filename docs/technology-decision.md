# Technology Decision

## Choice

Use **Rust as the only implementation core** and **npm as a distribution lane**.

## Why

- Rust fits a local control-plane binary well.
- npm lowers install friction for users who already work in the Node ecosystem.
- A TypeScript second core would duplicate runtime logic, release logic, and parity work.
- Comparable adjacent tools keep one underlying engine and then expose different
  surfaces; MCPace should do the same instead of splitting its core.

## Current implication

- `src/` is the source of truth for runtime behavior.
- `packages/npm/cli` is a thin launcher only.
- unsupported commands must fail clearly rather than pretending a removed bridge
  still exists.
- grouped `server`, `verify`, and `client plan` read paths are first-class Rust
  surfaces.
- one future entry point for many clients belongs in the hub control plane, not
  in client-specific one-off logic.

## Current proof boundary

This pass confirmed only the Node/source side again.
A Rust toolchain is required before claiming build proof for the current code.
