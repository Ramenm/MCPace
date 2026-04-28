# MCPace

`MCPace` is a Rust-first local MCP hub project.

The repository no longer ships PowerShell entrypoints. The active contract is a
native Rust CLI named `mcpace`, plus a thin npm launcher surface for users who
prefer npm-based installation.

This repo is intentionally honest about its state:

- implemented today: `version`, `doctor`, `setup`, `service`, `dashboard`, `serve`, `init`,
  `hub up/down/repair/status/logs`, `hub lease list/acquire/renew/release`, `profile show`, `projects list`,
  `candidates`, `client list`, `client plan`,
  `client install` / `client install all` (catalog-driven local config patcher with
  `--dry-run` / `--diff` previews and automatic restoreable backups; discover
  current write-capable surfaces via `mcpace client list --json`; Codex TOML
  installs also warn when preserved non-MCPace stdio entries point at missing
  programs that can fail client startup before MCPace runs),
  `client restore` (roll back the latest or named install backup for one client, or latest backups for all),
  `client export` (HTTP-first MCPace URL contracts for local clients,
  preview-only for blocked cloud/public surfaces), `lab list`, `lab matrix`,
  `lab coverage`, `lab gaps`, `lab report`, `lab show`, `server list`,
  `server capabilities`, `server candidates`, `verify doctor`,
  `verify readiness` (including non-mutating warnings for broken preserved
  Codex MCP commands), `repair`, `update check`, `release build` (local
  source/artifact/proof bundle only; no npm or GitHub publishing);
- internal compatibility surfaces kept for transition/debug work:
  `stdio-shim --json` (bootstrap-only proof surface) and `mcp-server`
  (stdio fallback lane with lease-gated upstream wrapper calls);
- the client catalog is now surface-aware and extensible: built-ins are a fallback, while `clientCatalog.targets`, `clientCatalog.paths`, and `MCPACE_CLIENT_CATALOG` can add or override local/cloud/API/generic surfaces without recompiling;
- the repo now includes a local file-backed hub lifecycle surface for bootstrap, state, health, logs, corruption repair, bounded log retention, and scheduler lease enforcement;
- release/platform automation is now prepared as CI workflows and package manifests, and
  `release build` wraps the local artifact/proof bundle while staying honest that publication is separate;
- planned next: broader pooled-session management, real config-writing `client export` for blocked cloud/public
  surfaces, richer upstream session fan-in, and transport-level cancellation/progress;
- stack policy is now explicit and machine-readable: Node 22/24 LTS contributor lanes, default local Node 24 via `.nvmrc` / `.node-version`, npm 10+, and a pinned Rust 1.95.0 toolchain are tracked in `docs/toolchain-policy.md` plus `reports/toolchain-support.json`;
- **not** reconfirmed in this pass: live Docker/runtime behavior, or multi-host parity on Windows/macOS/Linux.

## Current direction

MCPace is moving toward a **single local MCP hub for many clients** with one
public binary and one user-facing command taxonomy.

Today the repo contains:

- Rust source under `src/`;
- npm launcher packaging under `packages/npm/cli`;
- schema, examples, docs, reports, and repo-contract tests;
- **no** active PowerShell runtime layer.

Unsupported commands are reported as **not implemented yet in the Rust-only
repo** instead of silently bridging to deleted scripts.

## Current product truth

For the current cycle, the honest release promise is:

**one local MCPace endpoint, simpler install on selected local clients, and
honest diagnostics for what is configured versus actually usable.**

That means:

- first ICP: advanced integrator / solo power user with 2–3 local MCP clients;
- activation proven today: `setup` (or manual `serve start` + `client install`
  / `client export`), then `initialize -> tools/list` against
  `http://127.0.0.1:39022/mcp`;
- current product shape: local-first control plane plus onboarding layer with a
  connectable preview runtime surface;
- proof-tier-selected surfaces for the next cycle are resolved from the loaded client catalog entries marked `proofTier = tier-1`, and install-capable local surfaces are resolved from catalog metadata instead of hand-maintained client lists;
- machine-readable truth lives in `docs/product-truth.json` and `eval/runtime-capabilities.json` so reports and docs can share the same support language;
- cloud/public relay lanes remain preview or blocked until real runtime and
  host proof exist.

The operational version of this contract lives in
`docs/product-truth-and-beta-gate.md`; the routing/scheduler model lives in
`docs/universal-runtime-policy.md`.

## Native commands available now

These commands are implemented directly in Rust source:

```bash
mcpace version
mcpace doctor
mcpace setup --json
mcpace setup --json --install-service
mcpace service status --json
mcpace service install --json
mcpace service uninstall --json
mcpace service print --json
mcpace dashboard
mcpace serve --port 39022
mcpace serve start --json
mcpace serve status --json
mcpace serve stop --json
mcpace init --json
mcpace hub status --json
mcpace hub repair --json
mcpace hub logs --json --tail 20
mcpace hub lease list --json
mcpace hub lease acquire --json --server browser --client-id codex --session-id demo-1 --project-root /work/project-a
mcpace hub lease renew --json --lease-id <lease-id> --ttl-ms 120000
mcpace hub lease release --json --lease-id <lease-id>
mcpace stdio-shim --json --client-id codex --session-id demo-1 --project-root /work/project-a
mcpace mcp-server --root /work/project-a --client-id codex
mcpace repair --json
mcpace profile show --json
mcpace projects list --json
mcpace candidates --json
mcpace client list --json
mcpace client plan --json --client-id codex --session-id demo-1 --project-root /work/project-a
mcpace client install all --dry-run --diff --json
mcpace client install all
mcpace client install codex
mcpace client restore codex --backup latest
mcpace client restore all --backup latest
mcpace client install claude-code
mcpace client install cursor-local
mcpace client install kiro-ide
mcpace client install kiro-cli
mcpace client install gemini-cli
mcpace client install hermes-agent
mcpace client install windsurf
mcpace client install github-copilot-cli
mcpace client export codex --json
mcpace lab matrix --json
mcpace lab report
mcpace server list --json
mcpace server capabilities --json --name browser
mcpace server candidates --json
mcpace verify doctor
mcpace verify readiness
```

The packaged npm launcher now fails fast on unsupported Node versions with a
clear message instead of trying to limp along below the declared Node 22+ floor.
It now also resolves an optional vendored binary from `packages/npm/cli/vendor/<target>/` before falling back to future platform packages.

To build a clean source archive with one meaningful root directory and no
`node_modules`, `.git`, caches, or build junk, run:

```bash
npm run archive:release
```

If you already built the current host binary and want the launcher/archive to be
self-contained for that target, stage it first:

```bash
npm run stage:vendored-binary
npm run verify:vendored-binary
npm run verify:npm-pack
npm run archive:release
```

To build one canonical source-release bundle with a fresh archive, verification
report, checksums, and a machine-readable artifact manifest in `dist/`, run:

```bash
npm run build:release-artifacts
```

That bundle is cleaned and rebuilt from scratch so stale local ZIPs do not leak
into `SHA256SUMS.txt`, and a fresh proof run also keeps
`reports/verification-latest.json` synchronized with the bundled snapshot. It emits:

- `dist/<project-name>-v<version>-<ddmmyy-hhmmss>.zip`
- `dist/verification-latest.json`
- `dist/SHA256SUMS.txt`
- `dist/release-artifacts.json`

To regenerate only the latest machine-readable verification artifact from
executed source/release checks in this environment, run:

```bash
npm run prove:report
```

That writes `reports/verification-latest.json` without pretending that missing
Rust/runtime proof has already passed, and it now records whether the current
target is self-contained via a smoke-verified vendored binary, source-build-only,
or blocked without both Rust and a vendored binary. The source-proof lane now
uses a tarball contract (`npm run verify:npm-pack`) rather than trusting a raw
`npm pack --dry-run` alone.

To emit SHA-256 checksums for a custom artifact set, run:

```bash
npm run generate:checksums -- --output-dir dist
```

`doctor/profile/projects/candidates/client-plan/lab/server/verify` now have
native Rust read paths, `init` seeds the runtime layout, `hub` provides a
local lifecycle/status/log/repair/lease surface, `client list` exposes the
verified/generic client target catalog with surface-aware local/cloud/API
distinctions, `serve` is the public one-port MCP surface, explicit upstream
wrapper calls now acquire/heartbeat-renew/release scheduler leases, cancel on
lost heartbeat, put settings-only servers under a conservative lease, and expose
active lease-session bookkeeping for restart/cancel hardening, `surface_manifest`
reports the exact native MCPace tools versus proxied upstream tools without
pretending upstream names are top-level native tools, and `lab`
turns runtime fixtures plus capability inventory into an explicit backlog.

## Local dashboard available now

MCPace now includes a local browser dashboard for the CLI-first runtime. Start
it with:

```bash
mcpace dashboard
```

MCPace prints a localhost URL such as `http://127.0.0.1:43125`. The dashboard
shows runtime readiness, hub status, server inventory, documented client
surfaces, recent logs, and safe quick actions for `hub up`, `hub down`, and
`repair`.

## One-command local setup available now

For the easiest local path, run:

```bash
mcpace setup --json
```

`setup` starts the background one-port MCPace server, patches supported local
client configs through `client install all`, runs `verify readiness`, probes
`/healthz`, and performs MCP `initialize` + `tools/list` against
`http://127.0.0.1:39022/mcp`. It reports warnings honestly: cloud/public
connector lanes still need a relay, and stdio launcher exports need `mcpace` in
`PATH` (or an absolute binary path).

Use `mcpace setup --skip-client-install --json` when you only want to start and
smoke-test the local endpoint without writing client config files.

Use `mcpace setup --install-service --json` when you also want user-level
autostart. This is explicit opt-in because it writes OS startup configuration.
The service entry stores the current executable as an absolute path, so the
autostart path does **not** depend on `mcpace` already being in `PATH`.

## Autostart and package-manager direction

Current low-maintenance autostart uses the Rust `auto-launch` crate instead of
handwritten Windows/macOS/Linux startup files:

- Windows: current-user startup via the user registry lane;
- macOS: user LaunchAgent;
- Linux/Ubuntu: user-level systemd.

The command surface is:

```bash
mcpace service status --json
mcpace service install --json
mcpace service uninstall --json
mcpace service print --json
```

For verification or CI, use `mcpace service install --json --dry-run` or
`mcpace setup --json --install-service --no-enable`; those paths prove the
contract without mutating real user startup settings.

Distribution stays one Rust implementation core:

- now: source builds plus the thin npm launcher package;
- next: self-contained GitHub Release archives and npm platform packages;
- later, after signed artifact and install-test proof: Homebrew, WinGet, and
  Debian/Ubuntu `.deb`/APT repository lanes.

Those package managers should install the binary into `PATH`, but local HTTP
clients can connect through the configured URL even before `PATH` is fixed.

## One-port local serve mode available now

If you want one local process and one port instead of a separate UI mental
model, start MCPace like this:

```bash
mcpace serve --port 39022
```

That gives you:

- `http://127.0.0.1:39022/` — browser UI
- `http://127.0.0.1:39022/healthz` — health/readiness JSON
- `http://127.0.0.1:39022/mcp` — local MCP HTTP endpoint

This keeps the first local product surface smaller: one process, one port, one
entry point.

If you want MCPace to manage that server as a background process, use:

```bash
mcpace serve start --json
mcpace serve status --json
mcpace serve stop --json
```

## Local client connection path available now

Local clients can now reach MCPace through one local HTTP endpoint. For Codex,
the default shared MCPace block looks like this:

```toml
[mcp_servers.MCPace]
url = "http://127.0.0.1:39022/mcp"
enabled = true
startup_timeout_sec = 20
```

If you want MCPace to write the default shared-scope block for you, run:

```bash
mcpace client install all --dry-run --diff --json
mcpace client install codex
mcpace client restore codex --backup latest
mcpace client restore all --backup latest
mcpace client install all
```

Use `--dry-run` to compute the same candidate patch without creating or writing
client config files, and add `--diff` to inspect the exact current-vs-candidate
config change before allowing MCPace to persist it. Diff output redacts
secret-like keys such as tokens, passwords, API keys, and auth credentials.
Real writes create a local install backup under the MCPace state root; restore the
latest one with `mcpace client restore <client> --backup latest`, undo all latest
install backups with `mcpace client restore all --backup latest`, or use the
`restoreCommand` returned by `--json`. Backups preserve the exact previous
client config so rollback is lossless; treat the local MCPace state root as
sensitive if client configs contain tokens or credentials.

MCPace chooses the broadest documented shared scope for each supported local
client. `client install all` walks the loaded client catalog, patches local install-capable targets, and skips manual/cloud surfaces. Today that means user or global config files for:

- `mcpace client install claude-code`
- `mcpace client install cursor-local`
- `mcpace client install kiro-ide`
- `mcpace client install kiro-cli`
- `mcpace client install gemini-cli`
- `mcpace client install hermes-agent`
- `mcpace client install windsurf`
- `mcpace client install github-copilot-cli`

You can also add the same server with the Codex CLI:

```bash
codex mcp add MCPace --url http://127.0.0.1:39022/mcp
```

`client install codex` patches only the MCPace-owned block in the shared
user-scoped `~/.codex/config.toml` file by default, and the preferred local
shape is one running MCPace server on port `39022`.

For the exact Codex startup and troubleshooting flow, including the
`initialize -> notifications/initialized -> tools/list` Streamable HTTP
handshake check, see `docs/codex-mcpace-guide.md`.

Internal compatibility note: `mcp-server` and `stdio-shim` still exist for
debugging and fallback work, but they are no longer the primary local product
surface. `mcp-server` can exercise the same lease-gated upstream wrapper calls
as the HTTP endpoint; `stdio-shim --json` remains bootstrap-only.

Compatibility aliases currently kept for a smaller migration gap:

- `project` -> `projects`
- `servers` -> `server list`
- `capabilities` -> `server capabilities`
- `check` / `probe` -> `verify doctor`
- `status` / `readiness` -> `verify readiness`

## Why `client plan` exists already

The current release promise is **one local MCP endpoint for selected clients**.
The future product promise is still **one entry point for many clients**.
That broader promise only works if the hub owns session routing and upstream
server arbitration instead of letting each client guess for itself.

`client plan` is the first native control-plane slice for that promise:

- resolve client/session/project identity from explicit flags, env, or metadata;
- show the single-entry-point contract for future client installers/exporters;
- compute server isolation and request-serialization strategy from server policy;
- warn when project-local or single-session servers would be unsafe to share.

## Why `lab` exists already

It is too easy for a project like this to blur three very different claims:

- what the current code can already inspect or plan;
- what a future live hub should do;
- what still has no proof at all.

`lab` keeps those separate by reading:

- production-like runtime scenarios in `eval/fixtures/runtime/`;
- a capability inventory in `eval/runtime-capabilities.json`.

That gives you concrete answers to:

- which scenarios are **covered now**;
- which are only **partially covered**;
- which are still **blocked** by missing runtime or adapter work;
- which next steps close the biggest number of gaps.

For prompt / agent work, the repo now also carries grounded seed evals plus a
scenario map, scoring rubric, and regression plan under `eval/`. Those files are
meant to catch unsupported certainty, fake ETA precision, and vanity-benchmark
drift before they reach the user-facing docs or workflow.

## Grouped command surface

The target public surface remains grouped and smaller than the legacy script set:

```bash
mcpace init
mcpace setup
mcpace dashboard
mcpace serve
mcpace hub up
mcpace hub repair
mcpace hub status
mcpace client install codex
mcpace client install claude-code
mcpace client install cursor-local
mcpace client install kiro-ide
mcpace client install gemini-cli
mcpace client install hermes-agent
mcpace client install windsurf
mcpace client install github-copilot-cli
mcpace client restore codex --backup latest
mcpace client restore all --backup latest
mcpace client export codex
mcpace server list
mcpace profile show --json
mcpace projects list --json
mcpace verify doctor
mcpace verify readiness
mcpace repair
mcpace update check --json
mcpace release build # local artifact/proof bundle only; does not publish
```

At this stage, `setup`, `service`, `dashboard`, `serve`, `init`, `hub`, top-level `repair`,
HTTP-first `client export`, safe `update check`, the catalog-driven local `client install`
patchers with dry-run/diff previews plus `client restore` rollback backups, and
lease-gated explicit upstream wrapper calls with heartbeat renewal, lost-lease cancellation, conservative settings-only leases, bounded in-process upstream session pooling behind `upstream_call` / `upstream_batch`, config-driven `toolPolicies`, advisory `upstream_policy_audit`, generated `upstream_policy_suggest` candidates, `surface_manifest` as the transparent MCP tool-surface contract, and active session counts in `hub lease list --json` are implemented in source. The small default tool list is therefore an explicit wrapper/proxy design rather than a hidden direct-passthrough claim. `stdio-shim --json` remains a bootstrap-only internal
compatibility lane, while `mcp-server` remains a stdio fallback/debug lane. The runtime capability inventory now keeps a separate
`claimStatus` field so docs can say `supported`, `control-plane-only`,
`bootstrap-only`, or `connectable-preview` without pretending those are all the
same thing. Config-writing `client export` for broader cloud/public client
families still fails clearly as **planned but not implemented yet**; `release build`
is implemented for local artifacts/proof only and intentionally does not publish
to npm or GitHub.

## Toolchain lanes

See `docs/toolchain-policy.md` for the support policy. In short:

- contributors and CI should use Node 22 LTS or Node 24 LTS;
- the default local development line is Node 24 via `.nvmrc` and `.node-version`;
- the repo expects npm 10+;
- the Rust toolchain is pinned in `rust-toolchain.toml`;
- runtime proof still requires real supported hosts.

## Install and verification surfaces

The long-term install lanes for the same Rust binary are:

- GitHub Release platform archives;
- npm launcher package `@mcpace/cli`;
- later optional package-manager surfaces such as Homebrew, WinGet, and
  Debian/Ubuntu `.deb` / APT repositories.

Current Rust dependencies are intentionally reviewed rather than avoided
outright: `auto-launch` owns OS autostart, `which` owns executable lookup, and
`serde_json` owns JSON parsing/printing behind MCPace's compatibility wrapper.

npm is a distribution surface, not a second implementation core.

## Spec baseline

The current checked MCP spec baseline is **2025-11-25**.

First-wave obligations:

- `stdio` transport;
- `Streamable HTTP` transport;
- HTTP `Origin` validation and localhost binding for local-only HTTP lanes;
- environment-sourced credentials for `stdio` lanes;
- stateful session routing across initialization, operation, and shutdown;
- cancellation/progress support awareness for long-running requests;
- no dependence on experimental tasks for the first correctness slice.

## Test and proof model

Treat proof layers separately:

1. **source proof** — manifests, schema/examples, repo-contract checks, docs/tests consistency;
2. **build proof** — `cargo build --release`, `cargo test`, `npm pack --workspace @mcpace/cli --dry-run`;
3. **runtime proof** — real host runs with Docker and supported transports;
4. **release proof** — repeatable artifacts and publish flow.

Passing one layer does not imply the others.

## Quick checks available in this repo

Useful verification commands in a toolchain-equipped environment:

```bash
cargo test
cargo build --release
npm test
npm run pack:npm:dry-run
```

Useful grouped checks after a successful Rust build:

```bash
./target/release/mcpace init --json
./target/release/mcpace setup --json --skip-client-install
./target/release/mcpace service print --json
./target/release/mcpace service install --json --dry-run
./target/release/mcpace dashboard
./target/release/mcpace serve --port 39022
./target/release/mcpace hub status --json
./target/release/mcpace client list --json
./target/release/mcpace client export codex --json
mcpace client plan --json --client-id codex --session-id demo-1 --project-root /work/project-a
./target/release/mcpace lab report
./target/release/mcpace server list --json
./target/release/mcpace verify doctor
./target/release/mcpace verify readiness
```

## Project control docs

- `TODO.md` — prioritized backlog with status, dependencies, DoD, risks, and ETA ranges
- `STATE.md` — current verified status, progress view, assumptions, and next steps
- `DECISIONS.md` — active project decisions, alternatives, consequences, and review triggers
- `docs/codex-mcpace-guide.md` — Codex local MCP startup and handshake guide
- `docs/install-autostart-distribution.md` — package-manager and autostart path
- `docs/library-simplification-audit.md` — handwritten-code areas that are good
  candidates for safe library replacement

## Repository layout

- `src/` — Rust CLI and read-path implementation; `client`, `hub`, `lab`, and `server` now use thin module roots with focused submodules
- `packages/npm/cli` — thin npm launcher for the Rust binary
- `schemas/` — config schema
- `examples/` — example hub configs
- `docs/` — active design/runtime/test/release documentation
- `eval/` — runtime lab fixtures plus prompt/agent eval governance files
- `reports/` — coverage, verification, and release-summary artifacts
- `tests/` — Rust tests and Node repo-contract tests
- root project-control docs — `TODO.md`, `STATE.md`, `DECISIONS.md`

## Honesty rules

- Do not claim PowerShell support: the PowerShell layer was removed from this repo.
- Do not claim multi-host runtime parity from Node/source proof alone.
- Do not claim Docker or cross-host runtime proof from local build/test proof alone.
- Do not claim public release readiness until build + runtime + release proof exist.
