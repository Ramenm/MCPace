# MCPace toolbox doctor
Project: `mcpace` v`0.5.9`
Status: `blocked`
GitHub paid plan required: `no`
## Recommended local commands
| command | use |
|---|---|
| `npm run verify:local:smoke` | fastest local sanity loop while editing |
| `npm run verify:local:source` | source snapshot proof before pushing or sharing a ZIP |
| `npm run verify:local:full` | runtime/native proof on a host with Cargo dependency access |
| `npm run verify:publish-decision` | single yes/no decision for public source snapshot vs native npm publication |
## Tooling summary
| tool | requirement | status | evidence |
|---|---:|---:|---|
| node | required | blocked | v18.20.4; expected >= 22.0.0 |
| npm | required | blocked | 9.2.0; expected >= 10.0.0 |
| cargo | required | pass | cargo 1.95.0 (f2d3ce0bd 2026-03-21) |
| rustc | required | pass | rustc 1.95.0 (59807616e 2026-04-14) |
| rustfmt | required | pass | rustfmt 1.9.0-stable (59807616e1 2026-04-14) |
| clippy | required | pass | clippy 0.1.95 (59807616e1 2026-04-14) |
| git | recommended | pass | git version 2.39.2 |
| cargo-nextest | recommended | warn | error: no such command: `nextest` |
| cargo-audit | recommended | warn | error: no such command: `audit` |
| cargo-deny | recommended | warn | error: no such command: `deny` |
| cargo-auditable | optional | warn | error: no such command: `auditable` |

## Next actions

- Install Node >= 22.0.0.
- Install npm >= 10.0.0.
- Install cargo-nextest for fast local Rust test loops.
- Install cargo-audit before public release.
- Install cargo-deny before public release.
- Install cargo-auditable for stronger binary auditability.