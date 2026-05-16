# MCPace docs

This packaged copy tracks repo version `0.6.5`.

MCPace is a Rust-first local MCP hub. It ships with no upstream MCP servers enabled by default and no Rust-hardcoded recommended upstream catalog; useful presets live in editable data files. Use `mcpace connect` as the read-only top-down guide. Configure user-supplied stdio MCP servers with `mcpace server presets`, `mcpace server starter`, `mcpace server install`, `mcpace server import`, `mcpace server add`, `mcpace server test`, `mcpace server enable` / `mcpace server disable`, and `mcpace server remove`, root `mcp_settings.json`, `mcp_settings.d/*.json`, `mcpSettings.includePaths` / `mcpSettings.includeDirs`, or `MCPACE_MCP_SETTINGS` / `MCPACE_MCP_SETTINGS_DIRS`; extend useful presets with `mcpPresets.includePaths` or `MCPACE_MCP_PRESETS`; add `mcpace.config.json` server policy only when you need extra routing, platform, or tool-risk metadata.

Start with:

- `../ROADMAP.md` for the public-facing roadmap and what to star/watch/fork for.
- `github-launch-playbook.md` for GitHub launch, repository settings, maintainer loops, and growth without overclaiming.
- `ideal-product-backlog.md` for the maximum-quality backlog ordered by product impact.
- `maintainer-playbook.md` for triage, release, issue, and contribution routines.
- `bug-lifecycle.md` for reproduce-first bug fixing, root-cause notes, regression guards, and runtime traces.
- `bug-hunting-and-fix-playbook.md`, `defect-taxonomy-and-labels.md`, and `maintainer-debugging-guide.md` for the maintainer bug-sweep operating model.
- `product-truth.json` for the machine-readable product promise.
- `mcp-spec-alignment.md` for the checked MCP baseline.
- `client-surface-matrix.md` and `client-metadata-routing.md` for client routing.
- `server-segmentation-and-auto-discovery.md` for server discovery and serialization.
- `test-strategy.md` and `verification-matrix.md` for checks.
- `adr/0004-source-only-mcp-env-isolation.md` for the source-only MCP env isolation decision.
- `adr/0005-ci-cache-and-upstream-diagnostic-redaction.md` for Cargo CI caching and stderr diagnostic redaction.
- `mcp-http-api-spec.md`, `universal-mcp-connectivity.md`, `security-review-20260501.md`, and `adr/0006`/`0008`/`0009`/`0015`/`0017`/`0018` for the current `/mcp` hardening, configurable ingress, source-registry contract, client-first connect guide, preset-first useful MCP install flow, and `adr/0019-install-readiness-and-boot-harness.md` for the install/readiness harness decision.

Run this first when wiring a client:

```bash
mcpace connect --json
mcpace connect cursor-local --server filesystem
```


`product-practice.md` describes what not to claim before Rust/runtime proof. `performance-verification.md`, `multi-client-runtime.md`, `adaptive-mcp-orchestration.md`, and `adaptive-edge-case-coverage.md` define the source-level performance smoke pass and the host-specific proof still required before release performance claims.

Install/readiness artifacts now include `reports/boot-harness-latest.json`, `reports/boot-harness-latest.md`, `reports/install-readiness-latest.json`, and `reports/code-inventory-latest.*`. Use these before claiming an install path is ready.

Run from the repository root:

```bash
npm test
npm run inventory:source
npm run inventory:project
npm run verify:boot
npm run verify:install-readiness
npm run verify:product-practice
npm run verify:defect-gates
npm run verify:bug-sweep
npm run verify:runtime-trace
npm run verify:performance
npm run verify:dashboard-chaos
npm run verify:experience
npm run verify:rust-quality
cargo fmt --all -- --check
cargo check --all-targets --locked
cargo test --all-targets --locked
```

Client-first source-only upstream example. Start with the read-only guide, then import or add, smoke-test, and only then export/install a client config:

```bash
mcpace connect
mcpace server presets
mcpace server starter --path . --dry-run
mcpace server starter --path .
mcpace server sources --json
mcpace server test filesystem --refresh --json
mcpace client export cursor-local --json
mcpace client install cursor-local --dry-run
mcpace server disable my-server --dry-run
mcpace server enable my-server --dry-run
mcpace server remove my-server --dry-run
```

Manual JSON example:

```json
{
  "mcpServers": {
    "my-server": {
      "command": "node",
      "args": ["path/to/server.js"],
      "env": { "EXPLICIT_VAR": "value" },
      "env_vars": ["TOKEN_FROM_PARENT_ENV", { "name": "LOCAL_TOKEN", "source": "local" }],
      "cwd": "/absolute/or/project-specific/path"
    }
  }
}
```

Stdio upstream children get a cleared environment plus a small process-launch baseline, MCPace runtime variables, and explicit `env` / local `env_vars` values only. This preserves generic MCP server support without forwarding every parent process secret by default.
Cache/session fingerprints hash explicit env values so plaintext tokens are not embedded in cache keys.
Upstream stderr included in errors is bounded and sanitized before display so diagnostics remain useful without intentionally echoing obvious credentials.
- [Runtime state, cache, restart, and reinstall lifecycle](runtime-state-cache-lifecycle.md) - lifecycle contract for config, state, cache, sessions, restart, and reinstall behavior.

- [System lifecycle hardening](system-lifecycle-hardening.md) - end-to-end install/runtime/restart/reinstall/uninstall and release contract.

- [Tool scale and reuse hardening](tool-scale-and-reuse-hardening.md)

- [Mixed upstream topology hardening](mixed-upstream-topologies.md)

- [Upstream fail-safe hardening](upstream-failsafe-hardening.md)

- [Tool exposure and call safety](tool-exposure-and-call-safety.md)

## Packaged source archive

This ZIP is a clean source archive. It intentionally excludes `.git`, `node_modules`, build outputs, caches, generated reports, and stale prebuilt binaries. Build the Rust binary locally before creating npm/platform release artifacts.

- [Performance verification](performance-verification.md) - source-level performance smoke checks and host-specific runtime proof requirements.

- [Developer operating mode](developer-operating-mode.md) - grounded task intake, multi-track analysis, eval governance, cautious high-risk answers, and side-effect boundaries for maintainer/agent work.
