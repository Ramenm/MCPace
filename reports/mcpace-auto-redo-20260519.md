# MCPace automatic install/profile redo — 2026-05-19

Generated: 2026-05-19T08:31:10Z

## Outcome

MCPace is now source-config/evidence-first for upstream MCP servers. The project snapshot does not ship a packaged upstream-server catalog and does not enable any upstream server by default. Registration writes reviewable settings fragments; profiling stays conservative until source evidence and safe probes justify more concurrency.

## What changed

- Removed the packaged upstream-server catalog from the project tree and release manifest; there is no static upstream grouping file in the snapshot.
- Replaced catalog-first onboarding with source-level auto-install planning in src/mcp_autoinstall.rs and server/install.rs.
- server install now accepts npm/PyPI/OCI/URL/local-command specs and writes reviewable mcp_settings.d fragments without spawning the upstream package during registration.
- Source profiling is evidence-first: transport, launcher, command/url, args, operator policy hints, safe initialize/tools-list probes, and runtime evidence determine scheduling.
- Remote Streamable HTTP is session-safe by default until explicit stateless evidence exists; local filesystem, git, database, memory/context, shell/browser, credential/API, and network-docs signals get distinct lock/pool boundaries.
- Kept tags as observed evidence signals, not as hardcoded catalog identity. The scheduler sees lock domains and risk signals, not fixed server families.
- Added live random MCP package probes and policy checks for npm, PyPI, Context7, and blocked canary classes.
- Cleaned docs/tests/reports so the current UX explains automatic install/profile and does not point users at a static upstream catalog.

## Environment and setup evidence

- OS: Debian GNU/Linux 13 (trixie), x86_64 Linux kernel 4.4.
- Shell/user: /bin/bash, running as root in the sandbox.
- Runtime: Node v22.16.0, npm/npx 10.9.2, corepack 0.32.0, Python 3.13.5, git 2.47.3, uv/uvx 0.10.0.
- Project declares .nvmrc/.node-version 24, package engines Node >=22/npm >=10, packageManager npm@11.13.0, rust-toolchain 1.95.0 minimal with clippy/rustfmt.
- No npm dependency install was required because the root and CLI launcher have no runtime dependencies.
- Rust toolchain install was attempted through the project-declared local-toolchain path, but cargo/rustc/rustup were unavailable and static.rust-lang.org DNS was blocked.

## Verification

| Check | Status | Detail |
|---|---:|---|
| Node syntax | pass | 140/140 files |
| Full Node contract tests | pass | 210/210 pass |
| npm CLI tests | pass | 3/3 files |
| Adaptive auto-profile audit | pass | {"profileCount": 0, "edgeCaseCount": 13, "stableCount": 8, "conservativeCount": 2, "legacyCount": 1, "statefulCount": 11, "statelessCount": 2, "staticCatalogPresent": false} |
| Adaptive worker plan | pass | {"planCount": 13, "runtimePlanCount": 0, "edgePlanCount": 13, "consentPlanCount": 5, "meteredPlanCount": 3} |
| MCP install scenarios | pass | 10/10 pass |
| Live npm stable probes | pass | {"total": 3, "ok": 3, "failed": 0, "skipped": 0, "allowedNonOk": [], "unexpectedFailures": [], "totalTools": 24, "policyMismatches": [], "byStatus": {"ok": 3}, "byKind": {"npm": 3}, "byPolicy": {"project-filesystem-single-writer": 1, "state-profile-single-session": 2}, "serverSideRequestMethods": {"roots/list": 1}} |
| Live PyPI probes | pass | {"total": 4, "ok": 4, "failed": 0, "skipped": 0, "allowedNonOk": [], "unexpectedFailures": [], "totalTools": 21, "policyMismatches": [], "byStatus": {"ok": 4}, "byKind": {"pypi": 4}, "byPolicy": {"local-utility-multi-reader": 1, "project-repo-single-writer": 1, "network-fetch-review": 1, "database-path-single-writer": 1}, "serverSideRequestMethods": {}} |
| Live Context7 probe | pass | {"total": 1, "ok": 1, "failed": 0, "skipped": 0, "allowedNonOk": [], "unexpectedFailures": [], "totalTools": 2, "policyMismatches": [], "byStatus": {"ok": 1}, "byKind": {"npm": 1}, "byPolicy": {"network-docs-multi-reader-review": 1}, "serverSideRequestMethods": {}} |
| Canary skip policy | blocked | {"total": 4, "ok": 0, "failed": 0, "skipped": 4, "allowedNonOk": ["context7", "code-runner", "openapi-mcp", "tavily"], "unexpectedFailures": [], "totalTools": 0, "policyMismatches": [], "byStatus": {"skipped-by-policy": 4}, "byKind": {"npm": 4}, "byPolicy": {"network-docs-multi-reader-review": 1, "disabled-dangerous-command-runner": 1, "network-openapi-review": 1, "credential-s |
| Vendored binary smoke | pass | binary executable and version checked |
| npm pack dry run | pass | vendored-binary-bundle @mcpace/cli@0.6.5 |
| Secret scan | pass | {"findings": 0, "critical": 0, "warnings": 0} |
| Supply-chain local audit | pass-with-warnings | {"total": 11, "passed": 6, "warnings": 5, "blockers": 0} |
| Tooling readiness | blocked | Rust/native tools unavailable in this container |
| Rust quality | fail | blocked because cargo/rustc are unavailable |
| Publish decision | blocked | public native publication blocked until Rust rebuild/proof exists |

## Live MCP package probes

| Package | Version | Kind | Status | Tools | Suggested policy | Signals |
|---|---:|---|---:|---:|---|---|
| @modelcontextprotocol/server-filesystem | 2026.1.14 | npm | ok | 14 | project-filesystem-single-writer | filesystem, mutable-or-destructive-tools |
| @modelcontextprotocol/server-memory | 2026.1.26 | npm | ok | 9 | state-profile-single-session | memory-or-context, mutable-or-destructive-tools |
| @modelcontextprotocol/server-sequential-thinking | 2025.12.18 | npm | ok | 1 | state-profile-single-session | memory-or-context |
| mcp-server-time | 2026.1.26 | pypi | ok | 2 | local-utility-multi-reader | local-utility |
| mcp-server-git | 2026.1.14 | pypi | ok | 12 | project-repo-single-writer | git-repository, mutable-or-destructive-tools |
| mcp-server-fetch | 2025.4.7 | pypi | ok | 1 | network-fetch-review | network-fetch |
| mcp-server-sqlite | 2025.4.25 | pypi | ok | 6 | database-path-single-writer | database, mutable-or-destructive-tools |
| @upstash/context7-mcp | 2.2.5 | npm | ok | 2 | network-docs-multi-reader-review | network-or-external-api, open-world-annotation |
| @upstash/context7-mcp | 2.2.5 | npm | skipped-by-policy | 0 | network-docs-multi-reader-review | install-blocked |
| mcp-server-code-runner | 0.1.8 | npm | skipped-by-policy | 0 | disabled-dangerous-command-runner | install-blocked |
| @ivotoby/openapi-mcp-server | 1.14.0 | npm | skipped-by-policy | 0 | network-openapi-review | install-blocked |
| tavily-mcp | 0.2.19 | npm | skipped-by-policy | 0 | credential-scoped-review | install-blocked |

## Known limitations

- cargo/rustc/rustup are not available in this container and static.rust-lang.org DNS is blocked, so the Rust binary was not rebuilt here.
- The vendored npm binary is still the previous 0.6.5 binary; source-level auto-install changes must be rebuilt on a Rust host before native publication.
- Remote HTTP upstream forwarding is inventoried/session-modeled, but real remote connector/runtime traces still need a Rust rebuild and live endpoint proof before public native release claims.
- Canary packages that require credentials, arbitrary code execution, or expensive external APIs are intentionally skipped or disabled by policy unless explicitly reviewed.

## Rust-host proof still required

- `cargo fmt --all -- --check`
- `cargo check --all-targets --locked`
- `cargo clippy --all-targets --locked -- -D warnings`
- `cargo test --locked`
- `cargo build --release --locked`
- `npm run verify:rust-quality`
- `npm run verify:publish-decision:release`
