# MCPace local tooling readiness
Project: `mcpace` v`0.6.5`
Status: `ready-with-warnings`
GitHub paid plan required: `no`
## Tools
| tool | requirement | status | evidence |
|---|---:|---:|---|
| node | required | pass | v24.15.0 |
| npm | required | pass | 11.4.2 |
| cargo | required | pass | cargo 1.95.0 (f2d3ce0bd 2026-03-21) |
| rustc | required | pass | rustc 1.95.0 (59807616e 2026-04-14) |
| rustfmt | required | pass | rustfmt 1.9.0-stable (59807616e1 2026-04-14) |
| clippy | required | pass | clippy 0.1.95 (59807616e1 2026-04-14) |
| git | recommended | pass | git version 2.51.0.windows.2 |
| cargo-nextest | recommended | warn | error: no such command: `nextest` |
| cargo-audit | recommended | warn | error: no such command: `audit` |
| cargo-deny | recommended | warn | error: no such command: `deny` |
| cargo-auditable | optional | warn | error: no such command: `auditable` |

## Next actions

- Install cargo-nextest for fast local Rust test loops.
- Install cargo-audit before public release.
- Install cargo-deny before public release.
- Install cargo-auditable for stronger binary auditability.
