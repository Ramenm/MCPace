# STATE

## Snapshot

MCPace is currently a **Rust-first local MCP hub project** with a strong read-path/control-plane surface and an honest source-proof loop, but it is **not yet a fully proven live runtime**.

What is confirmed in-repo right now:

- native Rust read surfaces exist for `version`, `doctor`, `init`, `hub`, `profile show`, `projects list`, `candidates`, `client list`, `client plan`, `lab`, `server`, `verify`
- `stdio-shim --json` now exists as a bootstrap-only proof surface that reuses planner/export logic, derives a sticky lease, and ensures the persistent hub is up
- `client install` now writes MCPace-managed config blocks for the catalog-declared local patchers surfaced by `mcpace client list --json`
- `client export` now emits connectable preview contracts for local HTTP-capable clients plus preview-only blocker output for blocked cloud/public surfaces instead of failing as entirely unimplemented
- the hub has a **local file-backed lifecycle shell** with status, logs, repair, and seeded runtime state
- the client catalog is **surface-aware** across local / cloud / API-connector / generic shapes
- the runtime lab has **production-like fixtures** plus a capability inventory and gap report
- source/archive/npm checks run in this container; Rust build/runtime proof does **not**
- `reports/verification-latest.json` can now be regenerated from executed source/release checks via `npm run prove:report`
- the verification snapshot now reports the current target packaging mode plus optional vendored-binary smoke proof instead of treating mere file presence as enough evidence
- this pass fixed a source-level regression in `client/context` and corrected readiness semantics so `verify readiness` does not report runtime-green merely because a config file exists
- this pass fixed client metadata `_meta` fallback so root / `params` / `payload` / `payload.params` hints are merged field-by-field, and added exhaustive four-depth precedence/permutation coverage in Rust source tests
- this pass added a vendored binary packaging lane so built host binaries can be staged into `packages/npm/cli/vendor/<target>/`, resolved by the npm launcher, and carried into clean release archives when present
- this pass hardened release proof with a tarball contract (`scripts/verify-npm-pack.mjs`), checksum generation (`scripts/generate-checksums.mjs`), stronger vendored-binary smoke checks, a hosted release workflow scaffold, and a Docker full-work proof script that derives the current version dynamically
- this pass added a canonical `scripts/build-release-artifacts.mjs` bundle builder so local/CI source releases are rebuilt into one clean directory with an archive, verification snapshot, checksums, a machine-readable artifact manifest, and a synced canonical verification report
- this pass added dynamic client catalog extensions and scheduler-visible routing keys for project-local, browser-profile, desktop/host-lock, credential, and bounded-parallel server lanes
- this pass added a file-backed `hub lease` admission controller with acquire/renew/release/list, expired lease pruning, stale lock recovery, MCP tool exposure, and regression tests for host-lock, project-local, and bounded-parallel conflicts

## Product truth for the current cycle

- **first ICP:** advanced integrator / solo power user juggling 2–3 local MCP clients and tired of hand-maintained config drift
- **current public promise:** one local MCPace endpoint, selected local client install paths, and honest diagnostics for configured-vs-usable state
- **activation proven today:** `client install` or `client export`, then a real client reaches `http://127.0.0.1:39022/mcp` and completes at least `initialize -> tools/list`
- **beta-only activation still missing:** a real upstream tool call forwarded through MCPace's process/session manager with correct session/project ownership and no stale-result confusion
- **entrypoint contract:** `serve` is the product, `hub` is lifecycle machinery, `dashboard` is an optional view into state
- **proof-tier gate for the next cycle:** any client surface marked `proofTier = tier-1` in the loaded client catalog
- **truth taxonomy now split in the capability inventory:** `status` tracks full implementation completion, while `claimStatus` records the strongest honest public claim (`supported`, `supported-local-only`, `control-plane-only`, `bootstrap-only`, `connectable-preview`, `planned`)
- **machine-readable product truth:** `docs/product-truth.json` mirrors the current promise, activation, entrypoint contract, the catalog-driven proof-tier selector, and the catalog-driven install-support selector for report generation and doc drift checks
- **explicitly outside the current promise:** universal upstream runtime already proven, public relay lane already supported, or team-wide control plane guarantees

## What is done

- Removed legacy shell-runtime dependence from the active repo contract.
- Implemented grouped Rust command families and kept large families split into thin module roots.
- Added `client plan` so routing, project-root resolution, server policy arbitration, and surface constraints are visible before live runtime exists.
- Added config-writing `client install` for the supported local client surfaces while keeping `client export` preview-only where config patching is still blocked.
- Added bootstrap-only `stdio-shim --json` so the repo now has a real entrypoint for normalized session bootstrap and persistent-hub attach proof, without pretending that live MCP stdio forwarding already works.
- Added `lab list/matrix/coverage/gaps/report/show` so runtime claims are separated into covered / partial / blocked instead of being blurred together.
- Added top-level `repair` as a grouped maintenance shorthand over `hub repair` with native Rust integration coverage.
- Added a clean source archive builder and repo-contract tests for archive shape, doc drift, schema/example drift, stack drift, and fixture/capability parsing.
- Added project-control docs plus a more production-like prompt/agent eval governance set with machine-checked contracts.
- In this pass, restored missing planner helpers (`resolve_string`, `clean_optional_string`, `resolve_session_lease`) and the missing `CapabilityGap` import that would otherwise break Rust compilation.
- In this pass, tightened `doctor`/`verify readiness` so `runtime_prerequisites_ready` only turns green when required runtime prerequisites are actually present; today that means container-backed runtime servers imply Docker readiness.
- Added `scripts/proof-report.mjs` plus a repo contract test so the latest verification snapshot is generated from executed source/release checks and detected environment state.
- Synced package/manifests/reports/archive metadata to **0.3.6** and rebuilt the clean release archive.
- Added `scripts/stage-vendored-binary.mjs`, optional release-manifest vendor inclusion, and launcher-side vendored binary resolution ahead of optional platform packages.
- Added `scripts/verify-vendored-binary.mjs` and proof-report packaging-mode fields so self-contained current-target bundles can be smoke-checked instead of only detected.
- Added `scripts/verify-npm-pack.mjs`, `scripts/generate-checksums.mjs`, plus a hosted `release-artifacts` workflow scaffold so release packaging, checksums, and staged vendored-binary inclusion have machine-checked contracts before publication claims.
- Added `scripts/build-release-artifacts.mjs` plus contract coverage so canonical source bundles are rebuilt into a clean `dist/` set with `verification-latest.json`, `SHA256SUMS.txt`, and `release-artifacts.json` instead of depending on a potentially stale local artifact directory.
- Fixed `scripts/verify-ubuntu-docker-full.mjs` so the release-version proof derives the current project version instead of hardcoding `0.3.0`.
- Synced command-coverage/project-control artifacts with the real Rust surface and added Node drift contracts for those reports.

## What is in progress

- Converting the current control plane into a real runtime core: live `stdio` ingress, local Streamable HTTP ingress, lease-backed upstream forwarding, process-pool ownership, and cancel/stale-result guards.
- Keeping the eval suite tied to real maintainer work instead of vanity benchmarks.
- Tightening release/source proof so evidence paths, archive contents, and version alignment do not drift silently.

## What is blocked

- **Rust build proof** in this container is blocked because `cargo` / `rustc` are not installed here.
- **Runtime proof** is blocked because the current container is not a supported real-host environment for live Docker/client/transport validation.
- **Compatibility proof** for closed or cloud-only client surfaces is blocked until real traces or safe reproductions exist.
- **Published release proof** is blocked until GitHub Release and npm publish/provenance are exercised, not just documented.

## What happens next

1. Promote bootstrap-only `mcpace stdio-shim --json` into a live stdio forwarding path while keeping the reused planner logic as the single source of truth.
2. Add local Streamable HTTP ingress plus session handling.
3. Attach `hub lease` ownership to live upstream forwarding and add cancel/stale-result guards.
4. Re-run build/runtime proof on supported hosts.
5. Only then expand preview-only `client export` into real config patching for the still-blocked/public client lanes.

## Key metrics

### Verified repo metrics

- source-level native command surfaces: **34** (`reports/rust-command-coverage.md`)
- grouped command families implemented now: **7** (`client`, `hub`, `init`, `lab`, `repair`, `server`, `verify`)
- grouped commands still planned: **1** (`release`) plus the preview-only `client export` surface for blocked/public lanes
- runtime capability inventory: **24 total**
  - **13 implemented**
  - **11 planned**
  - public claim view:
    - **12 supported**
    - **2 supported-local-only**
    - **4 control-plane-only**
    - **1 bootstrap-only**
    - **1 connectable-preview**
    - **4 planned**
- runtime lab fixtures: **16**
  - **3 typical**
  - **9 edge**
  - **3 adversarial**
  - **1 held-out**
- seed prompt/agent fixtures: **21**
  - **3 typical**
  - **8 edge**
  - **8 adversarial**
  - **2 held-out**

### Progress view without fake precision

Two lenses are honest enough to use:

1. **Unweighted capability count**: `13 / 24` implemented = about **54%**.
2. **Public-claim mix**: `18 / 24` capabilities now have some honest non-planned claim, but most of that extra surface is still `control-plane-only`, `bootstrap-only`, `connectable-preview`, or `supported-local-only` rather than fully-proven runtime support.
3. **Coarse roadmap weighting**: roughly **45%–55% complete**.

Why the weighted view is a range rather than a single percentage:

- the already-finished work is biased toward **read-paths, planning, and source proof**
- many capabilities now have a meaningful `claimStatus`, but those intermediate states still stop short of the runtime proof needed for a stronger release promise
- the remaining work is biased toward **runtime correctness, host proof, and release proof**, which are heavier and riskier per item
- there is **no confirmed throughput history** in the repo, so any exact percentage would be artificial precision

### Point view

- done baseline: roughly **60–70 points**
- in-progress work: roughly **10–15 points**
- blocked + not-started work still ahead: roughly **55–75 points**

Interpretation: the repo is past the early bootstrap phase, but it is **not yet at release-candidate quality** because the hardest runtime/proof slices remain.

## Assumptions behind ETA and progress

- one focused maintainer
- no major architectural reset
- Rust toolchain and supported hosts become available when needed
- real-client trace capture is possible without secrets/PII leakage
- scope stays local-first; cloud relay/UI/desktop work remains deferred
