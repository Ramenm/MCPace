# Runtime lab harness

`mcpace lab` is the maintainer-facing proof surface for the automatic scheduler. It keeps the user flow simple (`mcpace auto`) while preserving a repeatable evidence chain for every policy decision.

## Default command

```bash
mcpace lab
mcpace lab --json
mcpace lab coverage
mcpace lab show --id popular-npm-filesystem
```

The default action is `report`. The report is intentionally phrased as:

```text
server -> evidence -> runtimeType/stateClass/effectClass -> concurrencyPolicy
```

## Corpus strategy

The lab corpus covers three groups:

1. Popular MCP servers inspected through package metadata downloaded in the sandbox with `npm pack` or `pip download`. The package files are not executed and are not shipped in the source archive.
2. Random and held-out registry-style records that represent remote Streamable HTTP servers, auth-gated SaaS servers, unknown npm stdio packages, and recognized-but-not-executable package types such as NuGet or MCPB.
3. A wider metadata sweep in `eval/package-metadata-sweep.json`, including browser automation packages, browser data packages, SaaS/admin APIs, database servers, documentation/search APIs, wallet/financial servers, and project-analysis tools.

The corpus lives in:

```text
eval/package-metadata-sweep.json
eval/popular-server-corpus.json
eval/runtime-capabilities.json
eval/fixtures/runtime/*.json
```

## Metadata layers

Each fixture declares the metadata layers that the auto-classifier is allowed to use:

- local approved catalog trust level;
- MCP Registry `server.json`: packages, remotes, transport, env, headers;
- package registry metadata: name, version, description, bin, keywords;
- downloaded package artifact metadata: `package.json`, Python `METADATA`, entry points, file list;
- README/description keyword scan using bounded semantic signals;
- safe `initialize` / `tools/list` probe for approved servers;
- tool names, descriptions, input schemas and annotations as untrusted hints;
- runtime observations: startup failure, crash, timeout, `tools/list_changed`, resources/prompts support;
- explicit user policy override as the final control.

Tool annotations are useful hints, but untrusted servers can lie. The classifier therefore treats annotations as supporting evidence only after catalog trust or an approved sandbox probe.

## What the lab proves

Each fixture declares the evidence that the auto-classifier must use:

- launcher/package source: npm, PyPI, OCI, remote URL, or recognized plan-only package type;
- transport: stdio, Streamable HTTP, or legacy SSE;
- surface signals from package metadata, README, package manifest and observed tool names;
- expected `runtimeType`, `stateClass`, `effectClass`, `concurrencyPolicy`, and `autoAction`;
- confidence, trust boundary and safe probe mode.

The report is not a replacement for live probing. It is a golden corpus that prevents accidental relaxations such as treating `github` as local `git`, sharing browser profiles, treating browser data packages like `caniuse` as interactive browser control, or running unknown random packages automatically.

## Browser split

Browser-related packages are intentionally split into three classes:

- local browser/session control: Playwright, Puppeteer, Chrome DevTools, existing Chrome-session bridges; these are `interactive` / `host-stateful` and use `single-session` host/profile locks;
- remote browser/session providers: Browserbase, BrowserStack and similar; these are `interactive` / `remote-session-stateful` and use credential/session locks, not local host locks;
- browser data/docs: caniuse and compatibility tables; these are `stateless` / `external-read`, not interactive automation.

## Safety boundary

The lab may download package metadata into a temporary sandbox for analysis, but it must not execute random server code. Unknown servers stay `plan-only` until a trust policy, local approval, and a safe probe path exist. Release bundles must include only the normalized fixtures and never package tarballs, wheels, caches, mirror URLs, or sandbox paths.

## Random held-out audit

`eval/random-server-audit.json` is a deterministic random-sample audit for packages that are intentionally not treated as first-party or pre-approved. It answers: "if a user discovers an unfamiliar MCP package, what evidence would MCPace use, and is the inferred runtime policy defensible?"

The audit currently includes browser-control, browser-observation, external-read, project-analysis, cluster-control and ambiguous web-crawl cases. The important split is:

- browser control: `@n8n/mcp-browser`, `@mcp-browser-kit/server`, Playwright browser managers -> `interactive` / `host-stateful` / `host-mutating` / `single-session`;
- browser observation: `@kazuph/mcp-browser-tabs` -> `stateful` / `host-stateful` / `read-only` / `multi-reader`, because it reads local browser tab state but does not click/navigate;
- browser data/docs: `@pipeworx/mcp-caniuse` -> `stateless` / `external-read`, because compatibility tables are not browser automation;
- external read APIs: Mapbox/search/docs/crawl packages -> budgeted `multi-reader` only when metadata and future `tools/list` evidence support read behavior;
- external admin/control: Kubernetes/Heroku/Azure/wallet/cluster packages -> credential-scoped `single-writer`;
- unknown or ambiguous stdio packages -> `plan-only` until approved and safely probed.

The random audit is metadata-only by default. It is designed to catch false positives, not to certify trust. `mcpace auto` may use its classification pattern, but it must still require trust/approval before executing unfamiliar package code.

## Random held-out npm sweep

MCPace also keeps a live-sampled held-out sweep in `eval/random-live-npm-sweep.json`. The sample is selected from `npm search --json "mcp" --searchlimit=250` with a deterministic SHA-256 seed, excluding packages that are already in the popular corpus. This avoids testing only well-known or hand-picked MCP servers.

The sweep records two classifications for each package:

- `commandOnly`: what MCPace can infer after a plain `npx -y <package>` style install, using only server name, command, URL, and args.
- `withProfileHints`: what MCPace can infer when dynamic discovery preserves package/catalog metadata such as title, description, package id, registry type, transport, and recommended mode as `mcpaceProfileHints`.

The important acceptance rule is not “everything becomes permissive”. The rule is:

1. metadata hints should reduce false `unknown` classifications;
2. weak or vague packages must stay `unknown-conservative`;
3. browser/control, browser-readonly, docs/search, project-analysis, SaaS/admin, database, mobile automation, and transport-gateway packages must not collapse into one generic bucket;
4. dependency names are not treated as trusted semantic evidence, because dependencies often contain generic HTTP/proxy/auth words unrelated to the actual tool behavior.

Unknown servers still stay `plan-only` until approved or safely probed. MCPace must not execute random server code just to classify it.

## Random 100 npm MCP sweep

`eval/random-100-npm-sweep.json` is the wider random audit. It uses three live npm searches (`mcp server`, `modelcontextprotocol`, and `mcp`), picks 100 packages with a deterministic SHA-256 seed, runs `npm pack` to download each package tarball, reads only package metadata/manifests, and never executes foreign server code.

The current sweep result:

- 100/100 package tarballs downloaded for metadata inspection;
- 95/100 classified into a concrete routing group;
- 5/100 kept as `unknown-conservative` because metadata was too weak;
- 0 mismatches between the script classification and the audit expectation;
- package tarballs are excluded from the release bundle.

The sweep forced two important classifier rules:

1. dependency names and README installation snippets are not trusted semantic evidence, because they create false positives such as treating SaaS packages as shell/process runners;
2. SDKs, middleware, examples, inspectors, and framework packages are classified as `package-artifact` / `not-a-server` / `plan-only` instead of being treated as runnable MCP servers.

Known conservative unknowns in the sweep are `@agentick/mcp`, `@milaboratories/pl-mcp-server`, `@vibeframe/mcp-server`, `@yjzf/mcp-server-yjzf`, and `terry-mcp`. These are safe outcomes: MCPace can discover them and show a plan, but should not auto-run or relax concurrency until package metadata, registry metadata, or a safe `initialize`/`tools/list` probe provides stronger evidence.

## Random 500 npm MCP sweep: second-pass review

`eval/random-500-reviewed-each-server.json` is the stricter audit that reviews every package from the 500-record sample. It exists because the first aggregate sweep was too optimistic: it compared one heuristic output to a similar heuristic expectation and therefore under-reported broad-signal mistakes.

The second-pass review keeps the raw sweep in `eval/random-500-npm-sweep.json`, then adds an independent per-server review layer with:

- previous classifier result;
- reviewed classifier result;
- evidence sources available for that package;
- confidence level;
- simplified automatic action;
- whether additional evidence is required.

The simplified user-facing actions are intentionally fewer than the internal runtime taxonomy:

| Action | Meaning |
|---|---|
| `static-safe-policy` | Metadata is strong enough to choose a conservative safe policy without asking the user. |
| `needs-safe-probe` | Metadata is useful but not enough; run `initialize` and `tools/list` in a sandbox before relaxing policy. |
| `plan-only` | Package looks like an SDK/client/framework/example/bridge artifact, not a runnable server. |
| `blocked-high-risk` | Metadata suggests shell, SSH, desktop, arbitrary command execution, malicious testing, or similar host-risk behavior. |

Current second-pass result over 500 sampled npm candidates:

- 500/500 records reviewed;
- 236 can receive a static conservative policy;
- 188 need safe `initialize` + `tools/list` probe before policy relaxation;
- 67 are plan-only artifacts;
- 9 are high-risk blocked-by-default candidates;
- unknown-conservative dropped from 148 to 88 after better metadata review;
- 207 records changed from the first broad-signal classifier, mostly because the old audit over-trusted words from README snippets, dependency names, and generic browser/process/project terms.

This is the main rule for future automation: static metadata can choose a conservative starting policy, but it cannot prove that a random server is safe to pool. Pooling and multi-reader relaxation require trusted catalog data or observed MCP surface evidence from `initialize` and `tools/list`.

## Name-free / indirect evidence policy

MCPace must not treat package names or server display names as trusted semantic evidence. Names are useful for search, install, UI labels and deduplication, but they are too noisy for runtime policy: random packages frequently include words like `browser`, `process`, `context`, `db`, `git`, or `server` even when the runnable surface means something else.

The classifier therefore separates identity from evidence:

| Evidence channel | Use | Can widen concurrency by itself? |
|---|---|---|
| package/server/install name | search, display, dedupe | no |
| transport and launcher shape | stdio vs remote HTTP, npx/uvx/Docker/url | no, only chooses initial conservative binding |
| package artifact manifest | runnable bin/entrypoint, SDK/example, dependency families, file layout | no, but can mark plan-only or high-risk |
| env/header requirements | credential/API/tenant binding | no, usually makes the policy more conservative |
| safe `initialize` probe | actual MCP compatibility and capabilities | not alone |
| safe `tools/list` probe | actual tool surface, descriptions, input/output schemas, annotations | yes, if read-only evidence is strong; annotations are still untrusted hints |
| `resources/list` and `prompts/list` | data/context surfaces without tool calls | sometimes, mostly to avoid over-classifying as mutating |
| runtime observations | startup timeout, crash, dynamic `list_changed`, stderr class | mostly lowers confidence or keeps conservative |

Automatic policy should follow this order:

```text
candidate identity
-> indirect metadata triage
-> package shape / env / dependency family
-> safe initialize + tools/list/resources/list/prompts/list if weak or conflicting
-> evidence score
-> simple action: static-safe-policy | needs-safe-probe | plan-only | blocked-high-risk
-> internal runtimeType/stateClass/effectClass only after evidence exists
```

A name-only server must become `needs-safe-probe` or `unknown-conservative`, not `multi-reader` or `pool`. This keeps future random MCP servers automatic without pretending that naming is reliable.

## Readiness boundary

`eval/auto-classification-readiness.json` is the current completion ledger. The current state is **core-ready-but-not-complete**:

- usable now: automatic metadata triage, approved/trusted auto setup, conservative unknown handling, lab corpus regression coverage, and name-free profile hints;
- not complete: live safe probe for low-confidence servers, runtime evidence score fields, tool schema effect parsing, generated permission manifests, multi-registry random sweeps, and drift/rug-pull detection.

The acceptance rule is intentionally strict: MCPace may choose a conservative static policy from indirect metadata, but it must not widen a random server to `multi-reader`, `pool`, or shared state until trusted catalog data or observed `initialize` + `tools/list` evidence supports that decision.


## Live safe probe

`mcpace lab probe` is the automatic escalation path for weak or random servers. It starts only configured servers and performs the safe MCP lifecycle: `initialize`, `notifications/initialized`, and `tools/list`. It does **not** run `tools/call`. Use it when static metadata produced `needs-safe-probe` or `unknown-conservative`:

```bash
mcpace lab probe --refresh --timeout-ms 30000
mcpace lab probe --id filesystem --refresh --json
```

The probe output is evidence, not blind trust: tool annotations are advisory, and MCPace still keeps conservative policy unless tool names, descriptions, schemas and configured policy agree.
