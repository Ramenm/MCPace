# Defect taxonomy and label model

This file defines the bug language for MCPace maintainers and contributors. The goal is searchable issues, predictable triage, and fixes that improve the product boundary instead of patching symptoms.

MCPace uses colon-separated labels because they are easy to type, sort, and mirror in `.github/labels.yml`.

## Type labels

- `type:bug`: confirmed product behavior is wrong.
- `type:regression`: worked in a previous version or commit.
- `type:flaky-test`: nondeterministic test, CI, fixture, timeout, or platform lane.
- `type:security`: security hardening, vulnerability, secret handling, auth/session boundary.
- `type:docs-drift`: docs, README, roadmap, or report claim no longer matches source behavior.
- `type:compatibility`: one client, platform, upstream, or protocol version behaves differently.
- `type:enhancement`: new capability or UX improvement.

## Area labels

- `area:cli`: command parsing, help, stdout/stderr, JSON output.
- `area:mcp-http`: local HTTP server, `/mcp`, `/healthz`, request limits, Host/Origin boundary.
- `area:upstream-stdio`: stdio launch, env isolation, tool list/call, stderr diagnostics.
- `area:upstream-http`: Streamable HTTP upstream inventory/fan-out, SSRF guard, auth isolation.
- `area:client-config`: client catalog, export/install, dry-run, diff, backup, restore.
- `area:npm`: npm launcher and npm package metadata.
- `area:release`: platform packages, checksums, GitHub Release, provenance.
- `area:docs`: docs, examples, schemas, reports.

## Severity labels

- `severity:p0`: security exposure, data loss, arbitrary command execution, corrupt rollback, runtime dead-on-arrival.
- `severity:p1`: core install/runtime path broken for supported users.
- `severity:p2`: important compatibility/runtime issue with workaround.
- `severity:p3`: paper cut, docs confusion, non-critical UX.

## Status labels

- `status:needs-repro`: issue has no minimal reproduction yet.
- `status:needs-root-cause`: reproduction exists, but the broken invariant is not named yet.
- `status:needs-runtime-trace`: code may be ready, but behavioral proof is missing.
- `status:needs-rust-proof`: source proof may be ready, but Cargo build/test/clippy proof is missing.
- `status:ready-for-review`: regression test and proof are attached.
- `full-ci`: issue/PR needs the full matrix, not only the source lane.

## Proof labels

Use status comments or PR checkboxes when GitHub labels would be too noisy:

- `proof:node-contract`: Node contract test added or updated.
- `proof:rust-test`: Rust unit/integration test added or updated.
- `proof:runtime-trace`: `reports/runtime-trace-latest.json` demonstrates the fix.
- `proof:real-client`: a real client trace demonstrates the fix.
- `proof:release-artifact`: package/release artifact proof demonstrates the fix.

## Triage order

1. Apply `type:*`, `area:*`, and `severity:*`.
2. Ask for missing version/platform/client/upstream details.
3. Move from `status:needs-repro` only with commands, fixture, or transcript.
4. Move from `status:needs-root-cause` only after the broken invariant is named.
5. Move from `status:needs-runtime-trace` only after the report or real-client trace is linked.
6. Close only after regression proof is linked.

## Duplicate and stale issue rules

- Keep the oldest issue with the clearest reproduction open.
- Link duplicates to the canonical reproduction.
- If no reproduction appears after two maintainer requests, close as `status:needs-repro` with a clear reopen condition.
- If a fix lands without fresh runtime/release proof, leave a follow-up issue labeled `status:needs-runtime-trace` or `status:needs-rust-proof` instead of upgrading product claims.
