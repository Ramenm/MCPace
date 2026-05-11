# MCPace boot harness

Generated: 2026-05-04T12:15:10.311Z

Project: `mcpace` v`0.5.9`

Install readiness: **pass**

## Toolchain

| tool | value | supported |
|---|---|---|
| node | v24.15.0 | yes |
| npm | 11.4.2 | yes |
| cargo | cargo 1.95.0 (f2d3ce0bd 2026-03-21) | yes |
| rustc | rustc 1.95.0 (59807616e 2026-04-14) | yes |

## Checks

| check | status |
|---|---|
| source inventory | pass |
| source audit | pass |
| node syntax | pass |
| npm pack | pass |
| binary distribution | vendored-binary-bundle |

## Next actions

- Run `cargo check --all-targets --locked` and `cargo test --all-targets --locked` on a host with Cargo dependency access.
- Record a real runtime trace: client -> /mcp -> initialize -> tools/list -> tools/call -> stdio upstream response.
