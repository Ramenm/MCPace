# Verification Matrix

## Source proof

- repo contains no `.ps1` files
- release manifest excludes deleted shell artifacts
- docs do not instruct removed entrypoints
- schema examples remain valid
- version numbers stay aligned across manifests and reports
- `client list` / `client plan` docs/tests align with the current grouped Rust surface
- contributor stack policy, local version files, package engines, and CI lanes stay aligned
- runtime fixtures and the capability inventory parse and satisfy the repo contract
- runtime capability inventory keeps `status` and `claimStatus` within the declared truth taxonomy
- runtime capability evidence paths and seed-eval evidence paths exist
- prompt/agent eval scenario map, rubric, and dataset plan parse and stay aligned with fixture ids
- thin module roots for large command families stay split instead of collapsing back into giant files
- `scripts/proof-report.mjs` can regenerate `reports/verification-latest.json` from executed source/release checks without overclaiming blocked proof layers

## Build proof

Requires a Rust-capable host:

- Rust binary builds successfully
- Rust tests run successfully
- npm launcher dry-run package succeeds

## Runtime proof

- `mcpace doctor` reports host prerequisites accurately
- `mcpace client plan` reports client, session, lease, and project resolution honestly
- `mcpace client plan` warns when project-local or single-session servers would be unsafe to share
- `mcpace lab matrix` reports runtime fixture coverage honestly
- `mcpace lab gaps` reports blocked capabilities honestly
- `mcpace lab report` turns fixture coverage into a prioritized backlog
- public docs and reports do not overclaim beyond each capability's `claimStatus`
- `mcpace server list` and `mcpace server capabilities` read real config/state honestly
- `mcpace verify doctor` and `mcpace verify readiness` work on real supported hosts
- later lifecycle commands work on Ubuntu, Windows, and macOS

## Release proof

- archive contains only intended files
- npm package contains only launcher files
- version numbers are aligned across manifests and reports
- CI includes Rust and Node validation before publication
- held-out runtime fixtures stay reserved for release gates
- held-out seed prompt/agent cases stay out of the day-to-day tuning loop
