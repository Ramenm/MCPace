# MCPace v0.6.9 source-bundle summary

## Product direction

This pass sharpens MCPace from “one MCP endpoint” into a local MCP process scheduler for concurrent AI agents.

The core niche is:

> MCPace turns single-session MCP servers into safe concurrent infrastructure.

That keeps the simple local endpoint, but makes the product different from generic MCP gateways: MCPace adapts fragile stdio/runtime behavior to the right concurrency mode for each client, project, or chat.

## What changed in this pass

- Fixed automatic policy inference for declared-but-policyless servers: they now inherit generic source inference instead of blank scheduler fields.
- Added a Node regression test covering declared server automatic policy inference.
- Added explicit server execution policy support through `mcpace server set-policy`.
- Added `mcpace server instances` to show the planned routing map, including traces such as `chat=A -> filesystem#1234abcd`.
- Added `mcpace server leases` as a friendly alias for active runtime leases.
- Added execution policy schema fields: `execution.mode`, `affinity`, `queueTimeoutMs`, `reusePolicy`, `maxWorkers`, and `maxInFlightPerWorker`.
- Added `executionDefaults`, `uiSurface`, and `approvedCatalog` sections to `mcpace.config.json`.
- Added an advisory local approved-server catalog at `catalog/approved-servers.json`.
- Added a sample stdio permission manifest at `manifests/filesystem.permissions.json`.
- Taught server profile loading to honor policy-level worker counts.
- Added tool-call and tool-batch audit events around actual upstream calls.
- Extended the dashboard overview with planned instances.
- Added dashboard cards for the concurrency map and audit trail.
- Repositioned the README around concurrency/runtime adaptation.
- Expanded architecture/configuration/runbook docs with concurrency modes, UI surface guidance, and the killer demo.
- Restored the npm CLI bin shim and report files required by the release manifest.

## Dynamic discovery / one-command auto pass

This follow-up simplifies dynamic discovery into one normal user-facing command:

```bash
mcpace auto
mcpace auto <query>
mcpace auto --dry-run
```

Advanced flags are still present for debugging, but the intended UX is now auto mode:

1. Refresh the MCP Registry cache when it is missing or stale.
2. Merge the approved local catalog and registry-style `server.json` metadata.
3. Infer package manager and transport from registry metadata instead of asking the user to choose a server type.
4. Build an install plan using the same planner as `mcpace install`.
5. Install only trusted/approved candidates automatically.
6. Probe live MCP `initialize` / `tools/list` evidence after install.
7. Feed the observed server into MCPace's conservative runtime policy inference.

Safety behavior remains deliberate:

- Configured new servers are picked up automatically from MCP settings.
- Approved/trusted catalog entries can be installed automatically by a no-query auto sweep.
- Registry results become install plans automatically, but review/unknown/blocked entries are not silently executed.
- NuGet and MCPB registry package types are recognized as registry package types, but remain plan-only until MCPace has a safe launcher for them; they are no longer misclassified as npm stdio packages.
- Direct real downloads from the official registry could not be completed in this sandbox because DNS resolution for `registry.modelcontextprotocol.io` failed. The code path is still wired and testable offline with registry-cache fixtures.


## Runtime state classification pass

This pass adds explicit automatic server classes instead of relying only on `scopeClass` and `concurrencyPolicy`:

- `runtimeType`: broad user-facing type (`stateless`, `stateful`, `external`, `interactive`, `side-effecting`, `legacy`, `unknown`).
- `stateClass`: scheduler partition (`stateless`, `session-stateful`, `project-stateful`, `credential-stateful`, `remote-session-stateful`, `host-stateful`, `unknown-conservative`).
- `effectClass`: call-effect posture (`read-only`, `external-read`, `ephemeral-state`, `project-mutating`, `external-mutating`, `host-mutating`, `process-exec`, `unknown`).

The classifier uses source names, commands, URLs, args, transport, configured tool policies, and policy overrides. Live MCP probes can later feed stronger evidence without requiring users to manually say whether a server is stateless or stateful.

## Architecture now intended

MCPace should stay local-first and lightweight:

1. Control plane: config schema, local catalog, permission manifests, profiles, CLI policy commands.
2. Runtime plane: leases, queueing, worker/session pools, stdio/HTTP adapters, audit logging.
3. UI/observability plane: local dashboard first; desktop tray later as a thin launcher/status wrapper.

The dashboard is the right first UI Surface because it can reuse `/api/overview` and does not add Electron/tray complexity while the runtime model is still maturing.

## Important files

- `README.md`
- `docs/README.md`
- `docs/architecture.md`
- `docs/configuration.md`
- `mcpace.config.json`
- `schemas/mcpace-config.schema.json`
- `catalog/approved-servers.json`
- `manifests/filesystem.permissions.json`
- `src/server/args.rs`
- `src/server/policy.rs`
- `src/server/instances.rs`
- `src/server.rs`
- `src/server/loader.rs`
- `src/upstream/lease_runtime.rs`
- `src/dashboard/overview.rs`
- `src/dashboard/index.html`
- `packages/npm/cli/bin/mcpace.js`

## Verification performed in this sandbox

Passed on Windows in this validation pass:

- `npm run check:ci` — Node lint, 89 Node tests, package boundary check, and release dry-run passed.
- `npm run check:rust` — `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and 113 Rust tests passed.
- `npm run build` — release binary built successfully.
- `npm run load:local -- --binary ./target/release/mcpace.exe --duration-ms 5000 --concurrency 64` — local serve load test passed with zero failed requests.
- `npm run pack:npm:dry-run` — npm package dry-run passed for `@mcpace/cli@0.6.9`.
- `npm publish --workspace @mcpace/cli --dry-run --json` — npm publish dry-run passed.
- `npm run build:release-artifacts` — source ZIP and manifest were generated and verified.

Publication status:

- The package name `@mcpace/cli` is not present in the public npm registry yet (`npm view` returned 404).
- This machine is not logged in to npm (`npm whoami` returned 401), so real npm publish still requires npm authentication.

## Package hygiene

The ZIP is a source bundle with one root directory. It excludes `.git`, `node_modules`, caches, temporary files, OS artifacts, runtime logs/data/backups, vendored platform binaries, Rust `target`, and other heavyweight build outputs.


## Additional runtime classification guardrails

- Replaced broad substring signal matching with token/boundary matching for server/package/command/tool clues.
- Prevented GitHub/GitLab-style API servers from being misclassified as local project `git` workers just because their name contains `git`.
- Prevented short mutation tokens such as `rm` from matching arbitrary words.
- Kept `maxWorkers=1` for single-writer and isolated-per-project servers; parallelism should come from separate project/session/process partitions, not concurrent calls into the same fragile worker.
- Dashboard statefulness no longer treats `parallelismLimit <= 1` alone as proof that a server is stateful.

## Runtime lab harness pass

This pass turns the earlier lab idea into a repeatable evidence corpus instead of another flag-heavy workflow.

User-facing auto mode stays simple:

```bash
mcpace auto
```

Maintainer proof mode is now:

```bash
mcpace lab
mcpace lab coverage
mcpace lab show --id popular-npm-filesystem
```

What changed:

- `mcpace lab` now defaults to a report instead of requiring a subcommand.
- Added `eval/runtime-capabilities.json` as a capability inventory for the automatic scheduler.
- Added `eval/popular-server-corpus.json` with sandbox-inspected package metadata for popular MCP servers.
- Added 18 runtime fixtures under `eval/fixtures/runtime/` covering popular npm/PyPI servers plus random/held-out registry cases.
- Extended lab scenarios with explicit expected `runtimeType`, `stateClass`, `effectClass`, `concurrencyPolicy`, `autoAction`, `serverArchetype`, and `evidenceSources`.
- Added `docs/lab-harness.md` and linked it from the runbook.
- Included `eval/` in the release manifest so the source bundle carries the golden corpus.

Sandbox package analysis performed without executing foreign MCP server code; not executing foreign MCP server code is the explicit safety boundary:

- `npm pack` downloaded metadata/packages for filesystem, memory, sequential-thinking, GitHub, Postgres, Puppeteer, Brave Search, Slack, Google Maps, and Everything servers.
- `pip download --no-deps` downloaded metadata/packages for fetch, time, git, and sqlite servers.
- The downloaded `.tgz` and `.whl` files are not part of the repository or release bundle; only normalized fixtures are shipped.

The lab now explicitly validates the intended proof chain:

```text
server -> evidence -> runtimeType/stateClass/effectClass -> concurrencyPolicy
```

Unknown random servers remain `plan-only` unless trust policy and safe probing allow more.


## Expanded lab sweep

The latest lab pass added a wider metadata sweep for real popular and random MCP servers. The sweep records 37 npm packages and 4 PyPI distributions in `eval/package-metadata-sweep.json`; downloaded package artifacts stayed outside the repository and are not shipped. The lab now tracks metadata layers explicitly: MCP Registry/server.json, package registry metadata, package artifact manifests, README/keyword signals, launcher/transport, trusted tools/list probe evidence, runtime observations, and user policy overrides.

New guardrails split browser-related servers into local browser sessions, remote browser sessions, and browser-data/documentation packages. This prevents `@pipeworx/mcp-caniuse`-style packages from becoming host-stateful browser automation while still keeping Playwright/Chrome/DevTools and random browser bridges serialized per profile. External read APIs such as Context7/Mapbox/search are budgeted multi-reader, while Notion/Sentry/Slack/GitLab/Kubernetes/Heroku/Azure/Phantom/Apify remain credential-scoped single-writer. Network database packages such as Postgres/Redis/Supabase are now database-connection scoped, while SQLite-style local databases remain project-scoped.

## Random MCP held-out audit

Added a deterministic random held-out audit in `eval/random-server-audit.json` and eight new runtime fixtures for unfamiliar MCP packages. This pass specifically checked whether MCPace can classify random browser/control, browser-observation, web-crawl, Mapbox, Kubernetes and ESLint-style packages without requiring the user to declare a server type.

Key improvement: read-only local browser observation is now split from browser automation. Packages such as `@kazuph/mcp-browser-tabs` are `host-stateful` because they read live browser state, but their `effectClass` is `read-only` and their policy can be `multi-reader` with a host-read lock. Browser automation packages such as `@n8n/mcp-browser`, `@mcp-browser-kit/server`, and Playwright browser managers remain `interactive` / `host-stateful` / `host-mutating` / `single-session`.

The audit remains metadata-only and does not execute random MCP server code. Unknown/random packages stay `plan-only` unless approved/trusted and safely probed with `initialize` and `tools/list`.


## Random npm MCP sweep

I ran a fresh held-out random npm sweep from `npm search --json "mcp" --searchlimit=250`, selected 20 packages with a deterministic SHA-256 seed, fetched package metadata with `npm view`, and did not execute foreign server code. The result is stored in `eval/random-live-npm-sweep.json`.

Findings:
- command-only classification left 4/20 packages as `unknown`;
- metadata-preserving profile hints reduced unknowns to 2/20;
- 6/20 packages materially changed classification when title/description/package metadata was retained;
- the remaining unknowns (`@z_ai/mcp-server`, `@extentos/mcp-server`) are intentionally conservative because metadata is too weak to prove behavior.

Fixes from this sweep:
- dynamic discovery now writes `mcpaceProfileHints` into MCP settings entries created by auto install;
- server loading reads `mcpaceProfileHints`/`profileHints` and feeds them into runtime policy inference;
- `desktop` and plain `stdio`/`transport` are no longer enough to misclassify generic packages as browser/session or gateway servers;
- Google Drive-style remote file APIs override local filesystem signals;
- generic `knowledge` no longer automatically means session memory.

## Random 100 MCP package audit

Ran a wider random npm sweep from three live queries: `mcp server`, `modelcontextprotocol`, and `mcp`. The audit selected 100 packages with a deterministic SHA-256 seed and downloaded 100 package tarballs with `npm pack` for metadata/manifest inspection only. No foreign MCP server code was executed, and the downloaded `.tgz` files are excluded from release artifacts.

Result: 95/100 random packages now receive a concrete automatic routing group, 5/100 remain `unknown-conservative`, and the audit records 0 mismatches between script output and audit expectation. The conservative unknowns are intentionally not auto-run because their metadata is too weak to prove runtime behavior.

Fixes from this pass:
- added `sdk-or-example` as an explicit non-runnable package-artifact class;
- added `plan-only`, `package-artifact`, `not-a-server`, and `not-runnable` schema values;
- expanded SaaS/admin/read-only design-docs signals for real packages such as Contentful, BrowserStack, Apify, Bitrix24, Databricks, MediaWiki, Netlify, Vendure, Transcend, and shadcn/Magic UI style servers;
- removed bare `process` as a shell/process signal so products such as Targetprocess no longer become false dangerous-process workers;
- kept SSH/terminal/exec style packages as dangerous-process/single-session.

## Random 500 npm sweep

This pass adds `eval/random-500-npm-sweep.json`, a deterministic held-out audit of 500 npm packages discovered through live `npm search` queries for MCP-related packages. The sandbox attempted real package downloads, but repeated `npm pack` calls can hang in this environment; therefore the report records the evidence available per package instead of pretending every package had the same evidence layer.

Recorded evidence:

- 500 packages from live npm search metadata.
- 130 packages with `npm pack --dry-run` file manifests.
- 49 packages with downloaded npm tarballs inspected for `package.json`, README and file list.
- 0 foreign MCP server packages executed.
- Downloaded `.tgz` files remained outside the repository and are not included in the release archive.

The sweep shows that package names/descriptions alone are not enough for many random packages. Unknown or low-confidence entries stay `unknown-conservative`; MCPace should only relax them after trusted catalog metadata or a safe `initialize` + `tools/list` probe. The new `eval/runtime-evidence-sources.json` documents the evidence layers used by the automatic classifier and which sources should be added before allowing a more permissive policy.

Classifier hardening added in response:

- A package that merely mentions “browser” is no longer browser automation unless it also has browser-control evidence such as Playwright, Puppeteer, DevTools, WebDriver, click/navigate/screenshot, or an explicit browser automation phrase.
- SDK/framework/example/library signals no longer force `plan-only` when the package clearly declares itself as a runnable MCP server.
- More SaaS/admin clues are recognized as credential-scoped providers instead of falling through to unknown.

## Random-500 second-pass classifier review

Added `eval/random-500-reviewed-each-server.json` and `.csv` as a per-server review layer over the previous 500 npm candidate sweep. The review found that the first sweep was too optimistic: 207 records changed after independent metadata review, including obvious broad-signal fixes such as Contentful moving from browser automation to credential-scoped SaaS/API.

New high-level action model:

- `static-safe-policy`: choose a conservative policy from metadata;
- `needs-safe-probe`: run safe `initialize` + `tools/list` before relaxing policy;
- `plan-only`: SDK/client/framework/example/bridge artifact;
- `blocked-high-risk`: shell/SSH/desktop/arbitrary-command style risk.

Second-pass counts: 236 static-safe, 188 needs-probe, 67 plan-only, 9 blocked high-risk, 88 remaining unknown-conservative. No foreign package code was executed; package artifacts remain excluded from release output.

## Auto-classification readiness pass

Added `eval/auto-classification-readiness.json` and a regression test to separate what is already usable from what is still missing for fully automatic random-server classification. Also tightened the Rust loader path so package/server names and raw package specs are not semantic runtime evidence: only indirect hints, safe flags, transport/auth shape, manifests, probes and runtime observations should drive policy.


## Final auto-readiness pass

Added `mcpace lab probe` as the live safe-probe escalation path for weak/random servers. The probe wraps the existing upstream MCP handshake and requests `tools/list` only; it does not call `tools/call`. Server profile evidence now exposes `evidenceScore`, `evidenceLevel`, `automaticAction`, and `nextStep`. Tool policy audit now reads input/output schemas for indirect path/sql/url/credential/command/mutation signals. The final readiness ledger is `eval/final-auto-pipeline.json`.
