# MCP Registry Lab Report

Schema: `mcpace.registryLab.v2`
Status: **pass**
Mode: `fixture-metadata-only`
Generated: 2026-05-19T12:36:11.178Z

This report is metadata-only. It does **not** install, launch, or call arbitrary third-party MCP servers.

## Summary

- Servers classified: 15
- Servers requiring review: 15
- Unknown default: review-required + single-writer + disabled-until-user-confirms

## Classifications

| Server | Decision | Scope | Concurrency | Risk signals | Confidence |
|---|---|---|---|---|---|
| io.modelcontextprotocol/filesystem | project-filesystem-single-writer | project-local | isolated-per-project | filesystem | medium |
| io.modelcontextprotocol/git | project-repo-single-writer | project-local | single-writer | git-repository | medium |
| io.github.example/playwright | shared-exclusive-host-lock | shared-exclusive | single-session | filesystem, browser-or-desktop | medium |
| io.github.example/context7 | network-docs-multi-reader-review | credential-scoped | multi-reader | network-open-world | medium |
| io.github.example/memory | state-profile-single-session | state-profile | single-session | memory-or-context | low |
| io.github.example/sqlite | database-path-single-writer | state-profile | single-writer | filesystem, database | low |
| io.github.example/shell-tools | disabled-dangerous-command-runner | host-global | single-writer | shell-or-process | high |
| io.github.example/unknown-widget | unknown-conservative-review | configured-source | single-writer | unknown-side-effects | low |
| io.github.example/azure-admin | cloud-admin-credential-review | credential-scoped | single-writer | cloud-admin | high |
| io.github.example/evm-wallet | blockchain-wallet-review | credential-scoped | single-writer | blockchain-wallet, secrets-or-credentials | high |
| io.github.example/prompt-trap | credential-scoped-stdio-review | credential-scoped | single-writer | prompt-injection-surface, secrets-or-credentials | low |
| io.github.example/stripe-billing | payments-financial-review | credential-scoped | single-writer | payments-financial | high |
| io.github.example/okta-admin | identity-admin-credential-review | credential-scoped | single-writer | cloud-admin, identity-admin | high |
| io.github.example/vault-secrets | secrets-manager-disabled-review | credential-scoped | single-writer | secrets-manager, secrets-or-credentials | high |
| io.github.example/gmail-mailbox | messaging-external-review | credential-scoped | single-writer | network-open-world, messaging-email, secrets-or-credentials | medium |

## Required next lanes

1. Sandbox launch with no user secrets, no user home directory, pinned package versions, clean environment, timeout, and process-tree kill.
2. Safe probe only: initialize and tools/list; no destructive tool calls.
3. Policy review comparing tool annotations, names, descriptions, transport, package registry, and user-provided trust.
4. Concurrency torture only for allowlisted fixtures and servers.

## Notes

- This lab is intentionally metadata-only. It never runs npx, uvx, docker, or arbitrary stdio commands.
- Registry metadata is discovery input, not trust proof. Unknown servers stay conservative until policy review.
- Sandbox probing and concurrency torture are planned follow-up lanes and must run without user secrets by default.
