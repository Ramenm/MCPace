## Summary

Describe the change and why it is needed.

## Validation

- [ ] `npm run check`
- [ ] `npm run pack:npm:dry-run`
- [ ] `npm run release:dry-run`
- [ ] `npm run check:rust`

## Windows / MCP notes

- [ ] npm/npx process launches use `.cmd` on Windows where needed.
- [ ] MCP stdio code writes only valid protocol messages to stdout; diagnostics go to stderr.
