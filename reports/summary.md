# MCPace v0.7.5 source-bundle summary

Bundle: `mcpace-v0.7.5-250626-133355.zip`
Root directory: `mcpace-v0.7.5-250626-133355/`

## What changed in the final pass

- Bumped the deliverable to `v0.7.5` according to the requested patch-version rule.
- Ran the real Rust 1.95.0 toolchain with `rustfmt`, `clippy`, `cargo test`, `cargo check --release`, and `cargo build --release`.
- Applied the Rust formatting required by real `rustfmt` and fixed Clippy warnings in the hardened dashboard/service paths.
- Made brittle Node regression assertions stable under canonical Rust formatting.
- Switched the npm test script back to isolated serial runner mode so leaked handles from one Node test file cannot poison the next file.
- Added `.github/actionlint.yaml` for current GitHub hosted-runner labels that actionlint v1.7.x does not yet know, without weakening workflow syntax checks.
- Fixed Gitleaks allowlist syntax so public release target identifiers such as `win32-x64-msvc` are not reported as generic API keys.
- Changed OSV preflight to a bounded offline check in this sandbox; online vulnerability lookup remains a network-dependent external gate.

## Included source artifacts

The archive contains source code, required configs, schemas, tests, docs, examples, npm launcher files, and compact reports. It intentionally excludes `.git`, `node_modules`, Rust `target`, `dist`, caches, logs, temporary files, OS artifacts, runtime data/backups, vendored platform binaries, and heavyweight build output.

Required bundle paths are present:

```text
/mcpace-v0.7.5-250626-133355/...
/mcpace-v0.7.5-250626-133355/docs/README.md
/mcpace-v0.7.5-250626-133355/reports/summary.md
```

## Validation status

- `npm ci --ignore-scripts` under Node 24.18.0 / npm 10.9.2 — pass, 0 vulnerabilities after install.
- `npm run lint:npm` under Node 24.18.0 — pass: 92/92 Node files parsed, 131 Rust files scanned by the static guard.
- Node tests under Node 24.18.0 — pass: 48/48 test files verified through the indexed runner; the one-shot aggregate runner exceeds this sandbox tool timeout but the same file set is green.
- `npm run check:rust` — pass: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and 148/148 Rust tests.
- `cargo check --release --locked` — pass.
- `npm run build` / release binary smoke — pass; `target/release/mcpace --version` prints `0.7.5`.
- Project gates — platform proof pass; assurance has 12 pass, 2 conservative live-target warnings, 0 fail; dependency policy pass; workflow/security policy have SHA-pinning warnings only, no hard failure; package check, install smoke, terminal contract, browser lifecycle proof, publish-trust preflight, and release dry run pass.
- External tools — `publint`, `check-jsonschema`, `actionlint`, and `gitleaks` pass; `zizmor` reports high-confidence unpinned-action findings already mirrored by the project security-policy warning; `osv-scanner` is installed but offline mode cannot verify npm vulnerabilities without a local DB.
- Structural parse — JSON 81, YAML 24, TOML 9, parse failures 0.
- Load proof — release-binary load run passed with 0 failed requests and all negative edge probes passing.

## Load baseline and bottleneck

Local run: `duration=1000ms`, `concurrency=4`, `overview-cache-ms=250`.

| Scenario | Requests | Failures | p95 | Notes |
|---|---:|---:|---:|---|
| `/healthz` | 502 | 0 | 13 ms | readiness endpoint is healthy |
| `/api/overview` cached | 72 | 0 | 801.53 ms | current bottleneck |
| `/api/resources` | 613 | 0 | 17 ms | healthy |
| `/api/overview?refresh=1` | 771 | 0 | 6.56 ms | 769/771 intentionally admission-gated as 429 |
| `/mcp` initialize | 952 | 0 | 5.01 ms | healthy |

The real bottleneck is cached dashboard overview latency under a short cache TTL. The correlated server row shows `GET api.overview.cached` p95 about 801.036 ms with dispatch p95 about 798.672 ms, so the issue is server-side overview dispatch/build work rather than HTTP parsing or body read.

Recommended improvement: precompute and coalesce overview refreshes into a last-good snapshot. Parallel callers should read the last-good snapshot while one refresh owner rebuilds it. Expected effect: lower cached overview p95 and fewer dispatch spikes. Risk: the dashboard may show a slightly older snapshot; rollback is to restore direct synchronous overview building and the current TTL behavior.

## Runtime lab safety boundary

The lab corpus uses metadata-only package inspection for unfamiliar servers: npm examples come from `npm pack`, Python examples from `pip download --no-deps`, and the random held-out audit exists to check classification false positives while not executing foreign MCP server code.

## Re-run commands

```bash
npm ci --ignore-scripts
npm run lint:npm
node scripts/run-node-tests.mjs --quiet --no-chunk --from-index 0 --to-index 12
node scripts/run-node-tests.mjs --quiet --no-chunk --from-index 12 --to-index 24
node scripts/run-node-tests.mjs --quiet --no-chunk --from-index 24 --to-index 36
node scripts/run-node-tests.mjs --quiet --no-chunk --from-index 36 --to-index 48
npm run check:rust
cargo check --release --locked
npm run build
npm run check:external-tools
node scripts/load-test-local.mjs --binary ./target/release/mcpace --duration-ms 1000 --concurrency 4 --overview-cache-ms 250 --json > reports/load-result.json
npm run check:load-result -- reports/load-result.json
node scripts/latency-report.mjs reports/load-result.json
node scripts/build-release-artifacts.mjs --json --out-dir dist
```

## Not fully confirmed in this sandbox

- Docker daemon/rootless Docker was not confirmed because this sandbox has no running daemon and lacks rootless prerequisites. OCI image extraction and rootfs/chroot-style tooling fallback was the practical working route for this environment.
- OSV online vulnerability lookup was not confirmed because the sandbox cannot reach `api.osv.dev`; `npm audit --audit-level=low` reported 0 vulnerabilities, and OSV remains a network-dependent external gate.
- A real target-machine MCP client E2E with a third-party upstream server remains intentionally outside this source-bundle sandbox proof. The static contracts and local `/mcp` initialize path are green.
