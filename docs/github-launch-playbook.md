# MCPace GitHub launch playbook

This is the public-repository readiness plan for making MCPace understandable, trustworthy, and attractive to developers.

## 1. Positioning

Use one clear sentence everywhere:

> MCPace is a Rust-first local MCP hub that gives many clients one local endpoint and safely brokers user-chosen upstream MCP servers.

The promise should stay narrow until the runtime proof expands:

- Current: local endpoint, BYO stdio upstreams, client config help, diagnostics, leases, install/export tooling.
- In progress: fresh proof for HTTP session lifecycle, fresher proof gates, native binary distribution.
- Planned: Streamable HTTP upstream fan-out, remote/public mode with real auth, broad client compatibility matrix.

This honesty is a feature. Developers trust projects that distinguish current support from north-star language.

## 2. README structure that earns stars

The README should answer five questions quickly:

1. What problem does this solve?
2. Can I try it in one minute?
3. What is safe by default?
4. What already works, and what is still planned?
5. How do I verify the claims myself?

Recommended top-of-README layout:

```md
# MCPace

One local MCP endpoint for many clients. Rust-first. BYO upstream MCP servers. Honest runtime proof.

[Install / Build] [Quickstart] [Status] [Security] [Roadmap]

## Why MCPace?
...

## First working path
...

## Current status
| Area | Status | Proof command |
...

## Safety model
...
```

When the GitHub repository name is known, add only real badges:

- CI workflow badge.
- Release workflow badge.
- npm package/version badge after the first publish.
- OpenSSF Scorecard badge after Scorecard is running on the public repo.

Do not add placeholder badges. Broken badges make a new project look abandoned.

## 3. Community files

Keep these files in the repo root or `.github/` so GitHub can include them in the community profile:

- `README.md`
- `LICENSE`
- `CONTRIBUTING.md`
- `CODE_OF_CONDUCT.md`
- `SECURITY.md`
- `SUPPORT.md`
- `.github/pull_request_template.md`
- `.github/ISSUE_TEMPLATE/bug_report.yml`
- `.github/ISSUE_TEMPLATE/feature_request.yml`
- `.github/ISSUE_TEMPLATE/repair-report.yml`
- `.github/ISSUE_TEMPLATE/cleanup-request.yml`

## 4. Security and supply-chain posture

MCPace sits between clients and MCP tools, so trust boundaries must be visible.

Minimum GitHub security posture:

- Dependabot for npm, Cargo, and GitHub Actions.
- Dependency review on pull requests.
- CodeQL for Rust, JavaScript, and GitHub Actions workflows where supported.
- OpenSSF Scorecard on default branch and on schedule.
- Private vulnerability reporting enabled in GitHub repository settings.
- Branch protection or repository rules requiring CI on `main`.
- npm trusted publishing with OIDC when packages are ready to publish.
- Release assets with checksums and attestations.

Product-specific security posture:

- No bundled upstream MCP servers enabled by default.
- Stdio child processes get a cleaned environment, not the full parent environment.
- Public/non-local mode must require auth.
- HTTP upstream URLs must have scheme/host/port validation and SSRF guards.
- Logs and proof reports must redact likely secrets.

## 5. Proof gates before public announcements

Do not announce “runtime beta” until these are green from a fresh checkout:

```bash
npm test
npm run verify:github-readiness
npm run verify:npm-pack
npm run verify:install-readiness
npm run verify:rust-quality
cargo build --release --locked
npm run stage:vendored-binary
npm run verify:vendored-binary
npm run verify:runtime-trace
npm run verify:product-practice
```

For release candidates, add:

```bash
npm run sync:platform-packages
npm run verify:platform-packages:packed
npm run verify:release-targets
npm run build:release-artifacts
npm run generate:checksums
```

The product-practice report should block runtime claims when reports are stale, from the wrong version, or from the wrong host binary.

## Launch sequence

This is the launch sequence MCPace should follow before broad public promotion.

## 6. GitHub launch sequence

### Before pushing public

- Remove private machine paths from reports or mark reports as historical examples.
- Make sure `mcp_settings.json`, `mcpace.config.json.servers`, and default candidate catalogs do not enable arbitrary upstream servers.
- Confirm `.gitignore` excludes state roots, logs, local configs, generated dist artifacts, and secrets.
- Run a fresh source inventory and source audit.
- Create a short terminal demo or GIF after the first clean runtime proof.

### First public push

- Push `main` with source, docs, workflows, and no vendored binaries unless the release strategy requires them.
- Enable private vulnerability reporting.
- Enable branch protection/repository rules.
- Enable Dependabot alerts and security updates.
- Verify all workflows run cleanly on GitHub-hosted runners.

### First release candidate

- Tag only after release-dry-run is green.
- Build platform artifacts through GitHub Actions, not local ad-hoc builds.
- Upload checksums and attestations.
- Publish npm packages through trusted publishing once repository metadata is exact.
- Update README with real install command and real proof badge links.

## Stars are earned

Stars are earned through clarity, usefulness, proof, and trust.

## 7. Star-growth checklist

Stars come from clarity plus trust, not just feature count.

- A memorable one-sentence promise.
- A quickstart that works for a new user.
- A visible demo.
- Honest status table.
- Clear safety model.
- Small useful automatic install examples for package, URL, and local command servers.
- Zero-surprise config mutation: dry-run, diff, backup, restore.
- Strong release hygiene.
- Issues that invite contribution: “good first issue”, “help wanted”, “runtime proof”, “client compatibility”.
- A roadmap that says what will not be done yet.

## 8. Source references checked

- GitHub community health files and community profile documentation.
- GitHub CodeQL and dependency review documentation.
- OpenSSF Scorecard documentation and GitHub Action.
- npm trusted publishing and provenance documentation.
- MCP Streamable HTTP transport and lifecycle documentation.

## Repository settings

Before broad promotion, enable branch protection for the default branch, require CI, enable Dependabot alerts, enable private vulnerability reporting, keep issue templates active, and configure npm trusted publishing only after release artifacts and repository metadata are proven.

## 9. Target users and contributor funnel

Primary wedge:

- advanced local MCP users;
- people using more than one MCP-capable client;
- people tired of duplicated MCP config;
- people who want dry-run, diff, backup, restore, and diagnostics before config mutation.

Good first issue categories:

- automatic install/profile examples for known upstream server shapes;
- docs examples for known upstream servers;
- clearer CLI error messages;
- compatibility trace fixtures;
- dashboard copy and empty states;
- tests for session/header edge cases.

Deeper contributor lanes:

- cross-process/relay-grade session persistence;
- HTTP upstream connector;
- lease ownership/cancellation hardening;
- platform package release proof;
- client patcher improvements.

## 10. Public metrics to track

- time to first successful `mcpace connect`;
- time to first `server test` pass;
- number of fresh runtime traces by OS/client;
- issues closed with proof added;
- stale report regressions caught by CI;
- release assets with checksums/provenance;
- client surfaces with rollback-tested install paths.
