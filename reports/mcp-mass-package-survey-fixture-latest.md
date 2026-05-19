# MCP mass package survey

Generated: 2026-05-19T12:36:12.246Z
Status: **pass**
Mode: fixture-replay

Packages: 100; high-risk: 47; install-lock ok: null; tarballs: 0.

## Safety

- Starts random MCP servers: false
- Calls MCP tools: false
- Allows install scripts: false
- Enables by default: false

## Packages

| Package | Version | Policy | State | Locks | Signals |
|---|---:|---|---|---|---|
| @upstash/context7-mcp | 2.2.5 | state-profile-single-session | session-stateful | session, context-store | memory-or-context |
| @apify/actors-mcp-server | 0.10.4 | credential-scoped-review | credential-session-stateful | credential-profile, tenant | credential-api, memory-or-context |
| @sentry/mcp-server | 0.33.0 | credential-scoped-review | credential-session-stateful | credential-profile, tenant | credential-api, memory-or-context |
| @notionhq/notion-mcp-server | 2.2.1 | credential-scoped-review | credential-session-stateful | credential-profile, tenant | credential-api |
| chrome-devtools-mcp | 1.0.1 | shared-exclusive-host-lock | host-context-stateful | browser-context, host-session | browser-or-desktop |
| @ui5/mcp-server | 0.2.11 | review-required-single-writer | unknown-stateful | server | unknown-side-effects |
| @hubspot/mcp-server | 0.4.0 | credential-scoped-review | credential-session-stateful | credential-profile, tenant | credential-api |
| @sap-ux/fiori-mcp-server | 0.7.0 | state-profile-single-session | session-stateful | session, context-store | memory-or-context |
| @railway/mcp-server | 0.1.11 | sensitive-admin-credential-review | credential-tenant-stateful | credential-profile, tenant | cloud-admin, memory-or-context |
| @modelcontextprotocol/server-filesystem | 2026.1.14 | project-filesystem-single-writer | project-stateful | file, project | filesystem, memory-or-context |
| @mapbox/mcp-server | 0.11.0 | review-required-single-writer | unknown-stateful | server | unknown-side-effects |
| @sigmacomputing/slack-mcp-server | 0.1.1 | credential-scoped-review | credential-session-stateful | credential-profile, tenant | credential-api |
| @heroku/mcp-server | 1.2.2 | sensitive-admin-credential-review | credential-tenant-stateful | credential-profile, tenant | cloud-admin |
| kubernetes-mcp-server | 0.0.62 | sensitive-admin-credential-review | credential-tenant-stateful | credential-profile, tenant | cloud-admin, memory-or-context |
| @eslint/mcp | 0.3.5 | review-required-single-writer | unknown-stateful | server | unknown-side-effects |
| @phantom/mcp-server | 1.2.7 | sensitive-admin-credential-review | credential-tenant-stateful | credential-profile, tenant | payments-or-wallet |
| @transcend-io/mcp-server-admin | 0.3.7 | review-required-single-writer | unknown-stateful | server | unknown-side-effects |
| mcp-server-kubernetes | 3.6.2 | sensitive-admin-credential-review | credential-tenant-stateful | credential-profile, tenant | cloud-admin |
| @motiffcom/motiff-mcp-server | 0.0.19 | review-required-single-writer | unknown-stateful | server | unknown-side-effects |
| @dynatrace-oss/dynatrace-mcp-server | 1.8.5 | state-profile-single-session | session-stateful | session, context-store | memory-or-context |
| @browserstack/mcp-server | 1.2.16 | shared-exclusive-host-lock | host-context-stateful | browser-context, host-session | browser-or-desktop |
| @winor30/mcp-server-datadog | 1.7.0 | credential-scoped-review | credential-session-stateful | credential-profile, tenant | credential-api, database |
| @supabase/mcp-server-supabase | 0.8.1 | review-required-single-writer | unknown-stateful | server | unknown-side-effects |
| @roychri/mcp-server-asana | 1.8.0 | review-required-single-writer | unknown-stateful | server | unknown-side-effects |
| @transcend-io/mcp-server-assessment | 0.3.8 | review-required-single-writer | unknown-stateful | server | unknown-side-effects |
| @z_ai/mcp-server | 0.1.4 | state-profile-single-session | session-stateful | session, context-store | memory-or-context |
| @currents/mcp | 2.3.1 | shared-exclusive-host-lock | host-context-stateful | browser-context, host-session | browser-or-desktop, git-repository |
| mcp-server-code-runner | 0.1.8 | disabled-dangerous-command-runner | host-process-stateful | host-session | shell-or-process |
| @coinbase/cds-mcp-server | 8.75.0 | sensitive-admin-credential-review | credential-tenant-stateful | credential-profile, tenant | payments-or-wallet |
| @esaio/esa-mcp-server | 0.8.1 | state-profile-single-session | session-stateful | session, context-store | memory-or-context |
| @transcend-io/mcp-server-workflows | 0.3.4 | review-required-single-writer | unknown-stateful | server | unknown-side-effects |
| @azure-devops/mcp | 2.7.0 | sensitive-admin-credential-review | credential-tenant-stateful | credential-profile, tenant | cloud-admin |
| @ivotoby/openapi-mcp-server | 1.14.0 | credential-scoped-review | credential-session-stateful | credential-profile, tenant | credential-api |
| @cloudflare/mcp-server-cloudflare | 0.2.0 | sensitive-admin-credential-review | credential-tenant-stateful | credential-profile, tenant | cloud-admin, credential-api |
| @aikidosec/mcp | 1.0.7 | review-required-single-writer | unknown-stateful | server | unknown-side-effects |
| @shortcut/mcp | 0.24.0 | state-profile-single-session | session-stateful | session, context-store | memory-or-context |
| @cap-js/mcp-server | 0.0.5 | state-profile-single-session | session-stateful | session, context-store | memory-or-context |
| @transcend-io/mcp-server-consent | 0.2.10 | review-required-single-writer | unknown-stateful | server | unknown-side-effects |
| slite-mcp-server | 1.3.0 | network-fetch-review | readonly-network-candidate | provider-budget | network-fetch |
| tavily-mcp | 0.2.19 | shared-exclusive-host-lock | host-context-stateful | browser-context, host-session | browser-or-desktop, credential-api, local-utility, memory-or-context, network-fetch |
| @theia/ai-mcp-server | 1.71.1 | review-required-single-writer | unknown-stateful | server | unknown-side-effects |
| @gongrzhe/server-gmail-autoauth-mcp | 1.1.11 | credential-scoped-review | credential-session-stateful | credential-profile, tenant | credential-api, memory-or-context |
| @taazkareem/clickup-mcp-server | 0.14.4 | shared-exclusive-host-lock | host-context-stateful | browser-context, host-session | browser-or-desktop, credential-api, memory-or-context |
| @onozaty/redmine-mcp-server | 1.2.0 | state-profile-single-session | session-stateful | session, context-store | memory-or-context |
| argocd-mcp | 0.7.0 | sensitive-admin-credential-review | credential-tenant-stateful | credential-profile, tenant | cloud-admin |
| @penpot/mcp | 2.15.0 | review-required-single-writer | unknown-stateful | server | unknown-side-effects |
| @microsoft/workiq | 0.4.1 | review-required-single-writer | unknown-stateful | server | unknown-side-effects |
| @alchemy/mcp-server | 0.3.0 | sensitive-admin-credential-review | credential-tenant-stateful | credential-profile, tenant | credential-api, memory-or-context, payments-or-wallet |
| hostinger-api-mcp | 0.2.1 | credential-scoped-review | credential-session-stateful | credential-profile, tenant | credential-api |
| @transcend-io/mcp-server-discovery | 0.3.4 | review-required-single-writer | unknown-stateful | server | unknown-side-effects |
| @superblocksteam/mcp-server | 2.0.101-next.1 | review-required-single-writer | unknown-stateful | server | unknown-side-effects |
| @postman/postman-mcp-server | 2.8.9 | credential-scoped-review | credential-session-stateful | credential-profile, tenant | credential-api |
| @azure/mcp | 3.0.0-beta.10 | sensitive-admin-credential-review | credential-tenant-stateful | credential-profile, tenant | cloud-admin, memory-or-context |
| @salesforce/mcp | 0.30.9 | credential-scoped-review | credential-session-stateful | credential-profile, tenant | credential-api |
| @extentos/mcp-server | 0.0.87 | state-profile-single-session | session-stateful | session, context-store | memory-or-context |
| polaris-mcp-server | 1.0.0 | review-required-single-writer | unknown-stateful | server | unknown-side-effects |
| @benborla29/mcp-server-mysql | 2.0.8 | database-path-single-writer | project-stateful | database, project | database, memory-or-context |
| mcp-hello-world | 1.1.2 | state-profile-single-session | session-stateful | session, context-store | local-utility, memory-or-context |
| @transcend-io/mcp-server-dsr | 0.3.8 | review-required-single-writer | unknown-stateful | server | unknown-side-effects |
| @transcend-io/mcp-server-inventory | 0.3.4 | review-required-single-writer | unknown-stateful | server | unknown-side-effects |
| @transcend-io/mcp-server-base | 0.4.3 | review-required-single-writer | unknown-stateful | server | unknown-side-effects |
| datadog-mcp-server | 1.0.9 | credential-scoped-review | credential-session-stateful | credential-profile, tenant | credential-api, database, memory-or-context |
| @github/computer-use-mcp | 0.1.27 | shared-exclusive-host-lock | host-context-stateful | browser-context, host-session | browser-or-desktop, git-repository |
| @ehrocks/fe-mcp-server | 1.0.6 | network-fetch-review | readonly-network-candidate | provider-budget | network-fetch |
| @modelcontextprotocol/server-sequential-thinking | 2025.12.18 | state-profile-single-session | session-stateful | session, context-store | memory-or-context |
| @storybook/mcp | 0.7.0 | state-profile-single-session | session-stateful | session, context-store | memory-or-context |
| graphlit-mcp-server | 1.0.20260112001 | disabled-dangerous-command-runner | host-process-stateful | host-session | credential-api, memory-or-context, network-fetch, shell-or-process |
| @xeroapi/xero-mcp-server | 0.0.16 | credential-scoped-review | credential-session-stateful | credential-profile, tenant | credential-api |
| @upstash/mcp-server | 0.2.3 | review-required-single-writer | unknown-stateful | server | unknown-side-effects |
| @bitwarden/mcp-server | 2026.2.0 | sensitive-admin-credential-review | credential-tenant-stateful | credential-profile, tenant | secret-or-identity |
| @brave/brave-search-mcp-server | 2.0.82 | credential-scoped-review | credential-session-stateful | credential-profile, tenant | credential-api, network-fetch |
| playwright-mcp-server | 1.0.0 | shared-exclusive-host-lock | host-context-stateful | browser-context, host-session | browser-or-desktop |
| @xyd-js/mcp-server | 0.0.0-build-df98432-20260513223339 | review-required-single-writer | unknown-stateful | server | unknown-side-effects |
| @variflight-ai/variflight-mcp | 1.0.3 | review-required-single-writer | unknown-stateful | server | unknown-side-effects |
| @transcend-io/mcp-server-preferences | 0.3.4 | review-required-single-writer | unknown-stateful | server | unknown-side-effects |
| @vendure/mcp-server | 1.0.4-alpha | review-required-single-writer | unknown-stateful | server | unknown-side-effects |
| codex-mcp-server | 1.4.10 | review-required-single-writer | unknown-stateful | server | unknown-side-effects |
| deepl-mcp-server | 1.1.0 | credential-scoped-review | credential-session-stateful | credential-profile, tenant | credential-api, network-fetch |
| wikipedia-mcp-server | 0.0.2 | network-fetch-review | readonly-network-candidate | provider-budget | network-fetch |
| malicious-mcp-server | 1.5.0 | state-profile-single-session | session-stateful | session, context-store | memory-or-context |
| mcp-server | 0.0.9 | review-required-single-writer | unknown-stateful | server | unknown-side-effects |
| @mantine/mcp-server | 9.2.1 | state-profile-single-session | session-stateful | session, context-store | memory-or-context |
| agentation-mcp | 1.2.0 | review-required-single-writer | unknown-stateful | server | unknown-side-effects |
| next-devtools-mcp | 0.3.10 | review-required-single-writer | unknown-stateful | server | unknown-side-effects |
| @tsmztech/mcp-server-salesforce | 0.0.6 | credential-scoped-review | credential-session-stateful | credential-profile, tenant | credential-api |
| @gleanwork/mcp-server-utils | 0.10.1 | review-required-single-writer | unknown-stateful | server | unknown-side-effects |
| @negokaz/excel-mcp-server | 0.12.0 | project-filesystem-single-writer | project-stateful | file, project | filesystem |
| openapi-mcp-generator | 3.3.0 | credential-scoped-review | credential-session-stateful | credential-profile, tenant | credential-api, memory-or-context |
| @amap/amap-maps-mcp-server | 0.0.8 | credential-scoped-review | credential-session-stateful | credential-profile, tenant | credential-api, network-fetch |
| @transloadit/mcp-server | 0.3.22 | disabled-dangerous-command-runner | host-process-stateful | host-session | filesystem, memory-or-context, shell-or-process |
| @siemens/element-mcp | 49.8.0-v.1.10.4 | review-required-single-writer | unknown-stateful | server | unknown-side-effects |
| storybook-mcp-server | 0.1.3 | shared-exclusive-host-lock | host-context-stateful | browser-context, host-session | browser-or-desktop, memory-or-context |
| terraform-mcp-server | 0.13.0 | sensitive-admin-credential-review | credential-tenant-stateful | credential-profile, tenant | cloud-admin |
| @nexus2520/bitbucket-mcp-server | 2.1.0 | credential-scoped-review | credential-session-stateful | credential-profile, tenant | credential-api, git-repository, memory-or-context |
| @softeria/ms-365-mcp-server | 0.110.0 | credential-scoped-review | credential-session-stateful | credential-profile, tenant | credential-api, memory-or-context |
| mcp-searxng | 1.0.3 | state-profile-single-session | session-stateful | session, context-store | memory-or-context, network-fetch |
| linkup-mcp-server | 3.2.0 | state-profile-single-session | session-stateful | session, context-store | memory-or-context, network-fetch |
| @mcp-apps/kusto-mcp-server | 1.0.47 | sensitive-admin-credential-review | credential-tenant-stateful | credential-profile, tenant | cloud-admin, database, memory-or-context |
| @circleci/mcp-server-circleci | 0.15.1 | sensitive-admin-credential-review | credential-tenant-stateful | credential-profile, tenant | cloud-admin, memory-or-context |
| @tocharianou/mcp-server-kibana | 0.8.0 | credential-scoped-review | credential-session-stateful | credential-profile, tenant | credential-api, memory-or-context, network-fetch |

## Checks

- PASS no-random-server-start: No random MCP package bins are started and no tools/call is sent.
- PASS install-scripts-disabled: All package-manager operations disable install scripts.
- PASS default-disabled: All surveyed packages remain disabled/not auto-enabled.
- PASS volume: Survey covers the requested MCP package volume.
- PASS locks-present: Every package has an explicit scheduling boundary.
