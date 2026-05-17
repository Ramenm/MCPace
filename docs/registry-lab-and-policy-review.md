# Registry lab and policy review

MCPace treats public MCP Registry metadata as discovery input, not trust proof. The lab lane is intentionally metadata-only by default: it can classify registry entries and propose reviewable policies, but it must not install, launch, or call arbitrary third-party MCP servers unless the operator explicitly enters a sandbox probe lane.

## Safety rules

- Unknown servers default to `review-required + single-writer + disabled-until-user-confirms`.
- Tool annotations are hints, not proof. Missing annotations are treated conservatively.
- Registry metadata can identify package type, version, transport, and installation information; it does not prove that the package code is safe.
- Live registry fetches contact only the MCP Registry API. The default fixture mode is fully source-only.
- Sandbox probes must run with a clean environment, no user secrets, no user home directory, pinned package versions, timeouts, and process-tree cleanup.

## Lanes

### A. Metadata-only classification

Command:

```bash
npm run verify:registry-lab
```

This reads `eval/fixtures/registry-sample.json`, writes `reports/registry-lab-latest.json` and `reports/registry-lab-latest.md`, and classifies server entries into reviewable policy buckets such as:

- `project-filesystem-single-writer`
- `project-repo-single-writer`
- `shared-exclusive-host-lock`
- `state-profile-single-session`
- `database-path-single-writer`
- `network-docs-multi-reader-review`
- `disabled-dangerous-command-runner`
- `cluster-admin-credential-review`
- `cloud-admin-credential-review`
- `blockchain-wallet-review`
- `network-openapi-review`
- `payments-financial-review`
- `identity-admin-credential-review`
- `secrets-manager-disabled-review`
- `messaging-external-review`
- `credential-scoped-stdio-review`
- `unknown-conservative-review`

Live metadata fetches are opt-in:

```bash
node scripts/registry-lab.mjs --live --limit 50 --json
```

### B. Sandbox launch

Planned next lane. Allowed actions are only `initialize` and `tools/list`. The lane must not pass user credentials, user home directories, broad filesystem mounts, or destructive tool calls.

### C. Policy audit

Compare registry metadata, package manager identity, transport, tool names, tool descriptions, annotations, server-side requests during discovery, credential requirements, prompt-injection-looking descriptions, and MCPace preset knowledge. The result is a suggested policy, never automatic trust.

### D. Concurrency torture

Run only against allowlisted fixtures or explicitly approved servers. Required scenarios:

- two clients against the same browser profile block or queue;
- two clients against the same git worktree serialize;
- two project roots isolate;
- memory/context servers keep session affinity;
- long-running calls can be cancelled and release leases;
- stale responses after timeout are ignored;
- crashed upstreams do not leak leases.

## UI requirements

The dashboard must stay minimal and answer five questions:

1. Is MCPace running?
2. Which clients are configured or visible?
3. Which servers are configured?
4. What is active, leased, blocked, or recently errored?
5. Which servers require policy review?

The UI must label unknown servers as untrusted and show the current policy in human language, for example `Project isolated`, `Single user at a time`, `Browser profile lock`, `Unknown, review required`, or `Disabled dangerous command runner`.

## Release gate

A release cannot claim safe random-server support unless all of these pass:

- source tests and version-drift checks are green;
- preset installs write explicit server policy overlays;
- unknown registry/server entries remain conservative;
- cloud, cluster, blockchain/wallet, command-runner, OpenAPI bridge, payment/financial, identity-admin, secrets-manager, and messaging/email entries remain disabled/review-gated by default;
- registry lab report is regenerated for the release version;
- sandbox probe lane exists and proves it does not pass user secrets;
- multi-client conflict tests prove lock/lease behavior;
- dashboard exposes Servers, Clients, Activity, and Policy Review without requiring users to inspect JSON.
