# Support

MCPace is pre-1.0 and proof-gated. Please include enough detail to reproduce a problem, and do not post secrets, private logs, or internal hostnames.

## Supported channels

- Bugs: use the bug report issue template.
- Feature ideas: use the feature request template and describe the user problem.
- Client/upstream compatibility: use the compatibility or runtime-proof template.
- Documentation gaps: use the documentation request template.
- Security vulnerabilities: follow `SECURITY.md`; do not open a public issue.

## Before opening an issue

Collect the smallest safe reproduction and include:

- MCPace version or commit.
- OS, architecture, shell, Node/npm versions, and Rust toolchain when relevant.
- Exact command or client action that failed.
- Selected client/upstream surface.
- Redacted command output, report JSON, or log excerpt.
- Whether the problem reproduces after `mcpace server test <server> --refresh --json`.

## Useful diagnostics

For most support requests, include redacted output from one or more of:

```bash
mcpace connect --json
mcpace server sources --json
mcpace server test <server> --refresh --json
mcpace verify readiness --json
npm run verify:product-practice
npm run verify:runtime-trace
```

## What to redact

Remove tokens, API keys, bearer headers, private keys, private file contents, internal hostnames, private project paths when necessary, and any personal data.

## Current support boundary

Until beta, support is focused on local-first source/build usage, selected local client install/export surfaces, stdio upstream smoke paths, and proof/report correctness. Public relay, broad remote MCP gateway behavior, and universal client compatibility are not supported claims yet.
