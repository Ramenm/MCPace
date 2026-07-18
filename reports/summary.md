# MCPace 0.8.2 verification summary

Generated: 2026-07-18T09:40:57Z
Source lineage: base `577f01ffd4075ea0f837eccc0953b103de16ec88` plus the 0.8.2 hardening changes in this release candidate

## Decision

The compact CLI, lifecycle, safe uninstall, source package, and local MCP path are **code-ready for review**. Production publication remains **NO-GO** until the exact resulting commit passes the disposable Linux/macOS/Windows matrix, Windows/macOS signing gates are satisfied, and the open HTTP identity/approval/lease/SSRF threat-model gates are resolved or explicitly accepted.

No real login item or user service was changed during the final local pass. Destructive login recovery remains double-gated and must run only in a disposable user, VM, or hosted runner.

## Product result

Public commands are now limited to:

```text
up  start  stop  restart  status  install  uninstall  advanced  help  version
```

Generated and installed compatibility contracts remain callable but hidden: `stdio`, `stdio-shim`, `agent run --autostart`, managed `serve`, `hub`, and `mcp-server`. `up` is convergent onboarding; upstream installation is exclusively `mcpace install`. Removed top-level commands, underscore spellings, pseudo-long single-dash options, redundant `advanced server add`, and `server candidates` fail instead of silently routing.

`status` is read-only and machine-readable. `uninstall` supports dry-run and ownership-aware removal while preserving package files, durable config, upstream definitions, and backups. Autostart replacement stops the previously registered supervisor before changing roots or commands. The lifecycle proof verifies manager target, endpoint, and process identity, restores prior state, and refuses destructive execution without both disposable-user gates.

## Exact local evidence

| Gate | Result |
| --- | --- |
| `npm test` | PASS — 73/73 files; direct TAP total 461 tests, 459 pass, 2 skip |
| `npm run check` | PASS |
| Rust tests | PASS — 306 library + 4 launcher tests |
| Enforced Rust proof run | PASS — current source via `scripts/cargo-task.mjs`; all test-server waits are bounded |
| `cargo fmt --all -- --check` | PASS |
| Clippy, all targets, locked/offline, warnings denied | PASS |
| `npm run lint:npm` and `publint` | PASS |
| `npm audit --omit=dev --ignore-scripts` | PASS — 0 vulnerabilities |
| Gitleaks release-manifest scan | PASS — 0 findings |
| Trivy final source ZIP scan | PASS — 0 HIGH/CRITICAL vulnerabilities, secrets, or misconfigurations |
| npm pack dry-run | PASS — 11 files |
| isolated installed-binary runtime smoke | PASS — `up`, health, MCP initialize/tools, `stop` |
| native Windows npm install smoke | PASS — launcher `0.8.2`, MCP tool count 8 |
| release/source artifact Node tests | PASS — 521-entry verified source archive |
| source release-readiness and endgame enforcement | PASS — 0 blockers |
| architecture boundary guard | PASS — 0 failures |
| legacy boundary guard | PASS — 0 unexpected files |
| live MCP stdio/HTTP lifecycle proof | PASS |
| Rust live proof and release build binding | PASS — 0 blockers |

Current release binary: `target/release/mcpace.exe`
SHA-256: `dbfc37e2cd4df61c346d116329af8dc469e77562eabcfce5b5335b2d3670b15a`
Rust source fingerprint: `30437034a9d96857215fa1b47f0fd9c67b418c81a24a4827a44a3da62a54e43a`

Machine-readable evidence:

- `reports/rust-live-proof.json`
- `reports/live-mcp-e2e-proof.json`

## External release blockers

1. Keep `scripts/autostart-lifecycle-proof.mjs --confirm-disposable-user` with `MCPACE_DISPOSABLE_AUTOSTART_PROOF=1` green on the exact release commit across Linux, Windows, and macOS disposable hosts.
2. Prove fresh login/reboot activation and recovery on those hosts; hosted macOS lifecycle is not a substitute for signed/notarized real GUI-login evidence.
3. Sign Windows binaries and sign, notarize, and staple macOS artifacts.
4. Enforce HTTP mutation authentication, principal-bound grants and leases, cancellation, centralized resolved-IP SSRF policy, and durable approval receipts according to an approved threat model.
5. Run the publication preflight from a clean exact-commit checkout and publish `0.8.2`; public npm `latest` was still `0.7.9` at the last registry check.
6. Re-run provenance and live proofs after any source change; these reports are not transferable to a later diff or commit.

## Safety boundary

The local final pass used isolated roots, mock servers, test processes, and source-level manager checks. It did not enable, disable, replace, or exercise the current user's real Windows login startup. The current workstation therefore does not provide the missing disposable-login proof.

Held-out package evidence remains metadata-only: npm artifacts are inspected with `npm pack`, Python artifacts with `pip download --no-deps`, and the random held-out audit is explicitly **not executing foreign MCP server code**.
