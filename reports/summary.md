# MCPace 0.8.2 verification summary

Generated: 2026-07-18T02:13:47+03:00
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
| `npm test` | PASS — 73/73 files; direct TAP total 457 tests, 455 pass, 2 skip |
| `npm run check` | PASS |
| Rust tests | PASS — 302 library + 4 launcher tests |
| Clean parallel Rust suite | PASS — 3/3 repeated runs via `scripts/cargo-task.mjs` |
| `cargo fmt --all -- --check` | PASS |
| Clippy, all targets, locked/offline, warnings denied | PASS |
| `npm run lint:npm` and `publint` | PASS |
| `npm audit --omit=dev --ignore-scripts` | PASS — 0 vulnerabilities |
| Gitleaks release-manifest scan | PASS — 0 findings |
| Trivy final source ZIP scan | PASS — 0 HIGH/CRITICAL vulnerabilities, secrets, or misconfigurations |
| npm pack dry-run | PASS — 11 files |
| isolated installed-binary runtime smoke | PASS — `up`, health, MCP initialize/tools, `stop` |
| native Windows npm install smoke | PASS — launcher `0.8.2`, MCP tool count 8 |
| release/source artifact Node tests | PASS |
| source release-readiness and endgame enforcement | PASS — 0 blockers |
| architecture boundary guard | PASS — 0 failures |
| legacy boundary guard | PASS — 0 unexpected files |
| live MCP stdio/HTTP lifecycle proof | PASS |
| Rust live proof and release build binding | PASS — 0 blockers |

Current release binary: `target/release/mcpace.exe`
SHA-256: `d9c0ad8be10da39eda7b4a45a1a1e417e896f37d80f1d27e79b3109863056c1f`
Rust source fingerprint: `4213ebcc4c200f034cbf7a5945ff512e0f37a3c860b5c5d7de9718f3d6cd210d`

Machine-readable evidence:

- `reports/rust-live-proof.json`
- `reports/live-mcp-e2e-proof.json`

## External release blockers

1. Run `scripts/autostart-lifecycle-proof.mjs --confirm-disposable-user` with `MCPACE_DISPOSABLE_AUTOSTART_PROOF=1` on exact-commit Linux, Windows, and macOS disposable hosts.
2. Prove fresh login/reboot activation and recovery on those hosts; hosted macOS lifecycle is not a substitute for signed/notarized real GUI-login evidence.
3. Sign Windows binaries and sign, notarize, and staple macOS artifacts.
4. Enforce HTTP mutation authentication, principal-bound grants and leases, cancellation, centralized resolved-IP SSRF policy, and durable approval receipts according to an approved threat model.
5. Run the publication preflight from a clean exact-commit checkout and publish `0.8.2`; public npm `latest` was still `0.7.9` at the last registry check.
6. Re-run provenance and live proofs after any source change; these reports are not transferable to a later diff or commit.

## Safety boundary

The local final pass used isolated roots, mock servers, test processes, and source-level manager checks. It did not enable, disable, replace, or exercise the current user's real Windows login startup. The current workstation therefore does not provide the missing disposable-login proof.

Held-out package evidence remains metadata-only: npm artifacts are inspected with `npm pack`, Python artifacts with `pip download --no-deps`, and the random held-out audit is explicitly **not executing foreign MCP server code**.
