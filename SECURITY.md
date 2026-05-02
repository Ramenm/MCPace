# Security Policy

## Reporting a vulnerability

Please report security issues privately instead of opening a public issue.

Use GitHub private vulnerability reporting when it is available for this
repository. If it is not available, contact the repository owner directly with:

- affected version or commit;
- reproduction steps;
- impact and required local capabilities;
- whether any credentials, local files, or MCP tool permissions are involved.

## Current security boundary

MCPace is a local MCP hub. Treat configured upstream MCP servers as trusted local
extensions unless their policy explicitly says otherwise. Do not enable upstream
servers or tool-risk allow flags for workflows you do not trust.

User-specific MCP server configuration belongs outside the repository, for
example in a user-owned file referenced by `MCPACE_MCP_SETTINGS`.

