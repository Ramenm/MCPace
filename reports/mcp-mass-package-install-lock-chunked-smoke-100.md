# MCP mass package survey

Generated: 2026-05-19T10:48:18.020Z
Status: **blocked**
Mode: live-npm-search-metadata

Packages: 100; high-risk: 34; install-lock ok: False; tarballs: 0.

## Safety

- Starts random MCP servers: False
- Calls MCP tools: False
- Allows install scripts: False
- Enables by default: False

## Checks

- PASS no-random-server-start: No random MCP package bins are started and no tools/call is sent.
- PASS install-scripts-disabled: All package-manager operations disable install scripts.
- PASS default-disabled: All surveyed packages remain disabled/not auto-enabled.
- PASS volume: Survey covers the requested MCP package volume.
- PASS locks-present: Every package has an explicit scheduling boundary.
- FAIL install-lock-resolution: npm install --package-lock-only did not complete within the configured safe budget; install scripts remained disabled and no MCP server was started.

## Blockers

- install-lock-resolution: npm install --package-lock-only did not complete within the configured safe budget; install scripts remained disabled and no MCP server was started.

## Notes

- Blocker wording regenerated after the mass-survey script was corrected; underlying safe install-lock result is unchanged.
