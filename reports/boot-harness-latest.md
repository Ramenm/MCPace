# MCPace boot harness

Generated: 2026-05-02T14:32:39.914Z

Project: `mcpace` v`0.5.9`

Install readiness: **partial**

## Toolchain

| tool | value | supported |
|---|---|---|
| node | v24.15.0 | yes |
| npm | missing | no |
| cargo | cargo 1.95.0 (f2d3ce0bd 2026-03-21) | yes |
| rustc | rustc 1.95.0 (59807616e 2026-04-14) | yes |

## Checks

| check | status |
|---|---|
| source inventory | pass |
| source audit | pass |
| node syntax | pass |
| npm pack | pass |
| binary distribution | thin-launcher |

## Warnings

- current npm missing is below project policy >=10.0.0
- no vendored/platform native binary staged; npm package remains a thin launcher/source-install artifact

## Next actions

- Use Node 22+ and npm 10+ for official source proof and install checks.
- Run `cargo check --all-targets --locked` and `cargo test --all-targets --locked` on a host with Cargo dependency access.
- Stage a native binary with `node scripts/stage-platform-package-binary.mjs ...` before claiming published npm install readiness.
- Record a real runtime trace: client -> /mcp -> initialize -> tools/list -> tools/call -> stdio upstream response.
