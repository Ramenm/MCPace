# Runtime lab harness

`mcpace lab` is the maintainer proof surface for automatic runtime scheduling. It keeps the user command simple (`mcpace auto`) while preserving a repeatable evidence chain:

```text
server -> evidence -> runtimeType/stateClass/effectClass -> concurrencyPolicy
```

## Default command

```bash
mcpace lab
mcpace lab coverage
mcpace lab show --id popular-npm-filesystem
mcpace lab probe --refresh --timeout-ms 30000
```

`mcpace lab` defaults to a report. The probe path is only for configured servers and performs `initialize`, `notifications/initialized`, and `tools/list`; it does not call upstream tools.

## Corpus strategy

The lab corpus lives under `eval/`:

| File or directory | Purpose |
|---|---|
| `eval/fixtures/runtime/*.json` | Golden server scenarios. |
| `eval/runtime-capabilities.json` | Scheduler capability inventory. |
| `eval/popular-server-corpus.json` | Popular npm/PyPI/registry packages inspected for metadata. |
| `eval/package-metadata-sweep.json` | Expanded metadata sweep. |
| `eval/random-server-audit.json` | Small held-out random audit. |
| `eval/random-live-npm-sweep.json` | Live-sampled npm sweep. |
| `eval/random-100-npm-sweep.json` | Wider 100-package npm audit. |
| `eval/random-500-npm-sweep.json` | Broad 500-package npm metadata sweep. |
| `eval/random-500-reviewed-each-server.json` | Independent second-pass review for each sampled package. |
| `eval/indirect-evidence-model.json` | Name-free evidence policy. |
| `eval/final-auto-pipeline.json` | Current readiness ledger. |

## Metadata layers

The lab records metadata layers separately instead of flattening everything into package names:

1. MCP Registry or catalog metadata;
2. package registry metadata;
3. package artifact manifests and file lists;
4. README/keyword signals;
5. launcher and transport shape;
6. environment/header requirements;
7. safe `initialize` and `tools/list` evidence;
8. optional `resources/list` and `prompts/list` evidence;
9. runtime observations and operator policy overrides.

Package names are useful for identity, search, install, and deduplication. MCPace must not treat package names or server display names as trusted semantic evidence, and name-only data is never enough for concurrency relaxation.

## What the lab proves

The lab checks that common server archetypes land in a safe default bucket:

| Archetype | Expected direction |
|---|---|
| Stateless utilities | `shared` or budgeted `pool`. |
| Session memory/thinking | `session-isolated`. |
| Filesystem/repo/database | project or database single-writer. |
| Credentialed SaaS/admin APIs | credential-scoped single-writer unless read-only evidence is strong. |
| Browser control | host-interactive and serialized/session-bound. |
| Browser observation | host-stateful but can be read-only multi-reader when evidence supports it. |
| Browser data/docs | external-read, not browser automation. |
| Shell/SSH/desktop/process | blocked or serialized by default. |
| SDK/example/framework packages | plan-only, not runnable servers. |
| Weak random stdio packages | needs safe probe or unknown-conservative. |

## Safety boundary

The lab may inspect package metadata and manifests, but it must not execute random server code. Unknown servers stay `plan-only` unless a trusted catalog, explicit approval, or safe probe result provides stronger evidence.

## Browser split

Browser wording is intentionally split:

- browser control: Playwright, Puppeteer, DevTools, click, navigate, screenshot, or automation APIs;
- browser observation: local tabs/history/session state read without navigation or mutation;
- browser data: compatibility tables, docs/search, or remote read APIs that happen to mention browsers.

This prevents browser data packages from becoming host-stateful automation and prevents browser observation from being treated as mutating control.

## Random held-out audit

`eval/random-server-audit.json` checks unfamiliar packages across browser control, browser observation, web crawl, Mapbox, Kubernetes, and ESLint-style project analysis. The audit is metadata-only and exists to catch false positives, not to certify trust.

Important rule: `mcpace auto` may use the audit's classification patterns, but it must still require trust or approval before executing unfamiliar package code.

## Random held-out npm sweep

`eval/random-live-npm-sweep.json` is selected from `npm search --json "mcp" --searchlimit=250` with a deterministic SHA-256 seed, excluding packages already present in the popular corpus.

It compares:

- `commandOnly`: inference from plain launcher shape such as `npx -y <package>`;
- `withProfileHints`: inference when dynamic discovery preserves description, registry type, transport, and recommended mode as profile hints.

Acceptance rules:

1. metadata hints should reduce false unknowns;
2. weak packages must stay `unknown-conservative`;
3. browser-control, browser-readonly, docs/search, project-analysis, SaaS/admin, database, mobile automation, and transport-gateway packages must not collapse into one bucket;
4. dependency names are not treated as trusted semantic evidence.

## Random 100 npm MCP sweep

`eval/random-100-npm-sweep.json` uses three npm searches (`mcp server`, `modelcontextprotocol`, and `mcp`), selects 100 packages with a deterministic seed, downloads package tarballs for metadata/manifest inspection, and never executes foreign server code.

Current documented result: 95/100 packages receive a concrete routing group, 5/100 stay `unknown-conservative`, and tarballs are excluded from the release bundle.

## Random 500 npm MCP sweep

`eval/random-500-npm-sweep.json` records broad coverage over 500 npm candidates. The first sweep showed that package names and descriptions are not enough for many random packages.

`eval/random-500-reviewed-each-server.json` adds the stricter second pass:

| Action | Meaning |
|---|---|
| `static-safe-policy` | Metadata is strong enough for a conservative starting policy. |
| `needs-safe-probe` | Metadata is useful but not enough; run `initialize` and `tools/list` before relaxing. |
| `plan-only` | Package looks like an SDK/client/framework/example/bridge artifact. |
| `blocked-high-risk` | Metadata suggests shell, SSH, desktop, arbitrary command execution, or similar host risk. |

The second pass keeps the broad sample useful without pretending that static metadata proves a random server is safe to pool.

## Name-free / indirect evidence policy

MCPace separates identity from policy evidence:

| Evidence channel | Use | Can widen concurrency alone? |
|---|---|---|
| Package/server/install name | Search, display, dedupe. | No. |
| Transport and launcher shape | Choose initial binding. | No. |
| Package artifact manifest | Identify runnable, plan-only, or high-risk shape. | No. |
| Env/header requirements | Bind to credential/API/tenant risk. | No. |
| Safe `initialize` probe | Confirm MCP compatibility and capabilities. | Not alone. |
| Safe `tools/list` probe | Inspect actual tool surface and schemas. | Yes, when read-only evidence is strong. |
| `resources/list` and `prompts/list` | Avoid over-classifying data/context as mutating tools. | Sometimes. |
| Runtime observations | Timeout/crash/list-changed/stderr class. | Usually lowers confidence. |

Automatic policy order:

```text
candidate identity
-> indirect metadata triage
-> package shape / env / dependency family
-> safe initialize + tools/list/resources/list/prompts/list if weak or conflicting
-> evidence score
-> simple action: static-safe-policy | needs-safe-probe | plan-only | blocked-high-risk
-> internal runtimeType/stateClass/effectClass only after evidence exists
```

A name-only server must become `needs-safe-probe` or `unknown-conservative`, not `multi-reader` or `pool`.

## Readiness boundary

`eval/auto-classification-readiness.json` is the current completion ledger. The state is core-ready-but-not-complete: automatic metadata triage, approved/trusted auto setup, conservative unknown handling, lab corpus regression coverage, and name-free profile hints are usable; low-confidence servers still need safe probe evidence before policy relaxation.

## Live safe probe

`mcpace lab probe` starts only configured servers and runs the safe MCP lifecycle. It does not run `tools/call`:

```bash
mcpace lab probe --refresh --timeout-ms 30000
mcpace lab probe --id filesystem --refresh --json
```

The output is evidence, not blind trust. Tool annotations are advisory, and MCPace keeps conservative policy unless names, descriptions, schemas, and configured policy agree.
