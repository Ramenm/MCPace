# Maintainer debugging guide

This guide is the command map for finding and removing bugs in MCPace. Use it with `docs/bug-hunting-and-fix-playbook.md`.

## Start with the cheap gates

```bash
npm run lint:npm
npm run verify:bug-sweep
npm run audit:source
npm run verify:github-readiness
```

These catch syntax drift, known risky source patterns, repo/issue-template drift, stale public claims, and missing GitHub/public-launch surfaces before runtime work begins.

## Then isolate by failing area

| Area | First commands | What to inspect |
|---|---|---|
| CLI/source | `npm run test:repo -- --timeout-ms 180000` | failing contract file, command output shape, docs drift |
| npm launcher | `npm run test:npm`, `npm run verify:npm-pack` | binary resolver, platform package manifest, thin launcher behavior |
| Rust source | `cargo fmt --all -- --check`, `cargo clippy --all-targets --locked -- -D warnings` | new warnings, panics, unchecked process boundaries |
| Runtime HTTP | `npm run verify:runtime-trace` after release build | `/mcp` initialize/session/tool flow, headers, request limits |
| Stdio upstream | `mcpace server test <name> --refresh --json` | env allowlist, command resolution, stderr redaction, timeout |
| Client install | `mcpace client install <client> --dry-run --diff` | patch plan, backup/restore, endpoint resolver |
| Release | `npm run build:release-artifacts`, `npm run verify:publish-readiness` | target matrix, checksums, provenance, trusted publishing metadata |

## Runtime HTTP checklist

When `/mcp` is involved, capture these explicitly:

- request method, path, Host, Origin, Accept, Content-Type;
- initialize body and negotiated protocol version;
- returned `Mcp-Session-Id` and `MCP-Protocol-Version`;
- missing/unknown/expired/protocol-mismatched session behavior;
- DELETE close behavior;
- whether the server is bound to localhost or a public interface;
- whether auth/relay mode is involved.

Boundary rules:

- Origin and Host must be local/allowed in local serve mode.
- `Mcp-Session-Id` must be server-minted, visible ASCII, bounded, and unpredictable.
- Stateful requests after initialize must include a known session id.
- Unknown/expired/closed sessions must force re-initialize instead of silently creating state.

## Upstream stdio checklist

- Does command resolution use the expected cwd/PATH?
- Is the child environment cleared and rebuilt from the allowlist?
- Are `env` and `env_vars` explicit?
- Are stderr diagnostics bounded and redacted?
- Does `tools/list` succeed before `tools/call`?
- Does the failure include actionable command/source information without secrets?

## Flaky test checklist

1. Re-run only the failing file with `node scripts/run-node-test-files.mjs --dir <dir> --ext <ext> --timeout-ms 180000`.
2. Check whether the test starts processes, opens ports, writes repo files, or depends on report freshness.
3. Add isolated temp dirs and explicit cleanup.
4. Prefer deterministic fixtures over sleeping.
5. If the test is genuinely slow, add progress/timeout diagnostics before raising timeout.

## Release/debugging rule

Never make a release claim stronger than the latest fresh proof report. If a report is old, host-incompatible, or generated before the current package version, treat it as evidence to investigate, not evidence to publish.
