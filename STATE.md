# STATE

## Snapshot

MCPace is currently a **Rust-first local MCP hub project** with a strong read-path/control-plane surface and an honest source-proof loop, but it is **not yet a fully proven live runtime**.

What is confirmed in-repo right now:

- native Rust read surfaces exist for `version`, `doctor`, `init`, `hub`, `profile show`, `projects list`, `candidates`, `client list`, `client plan`, `lab`, `server`, `verify`
- `stdio-shim --json` now exists as a bootstrap-only proof surface that reuses planner/export logic, derives a sticky lease, and ensures the persistent hub is up
- `client export` now emits a preview-only adapter contract with blockers and next actions instead of failing as entirely unimplemented
- the hub has a **local file-backed lifecycle shell** with status, logs, repair, and seeded runtime state
- the client catalog is **surface-aware** across local / cloud / API-connector / generic shapes
- the runtime lab has **production-like fixtures** plus a capability inventory and gap report
- source/archive/npm checks run in this container; Rust build/runtime proof does **not**
- `reports/verification-latest.json` can now be regenerated from executed source/release checks via `npm run prove:report`
- this pass fixed a source-level regression in `client/context` and corrected readiness semantics so `verify readiness` does not report runtime-green merely because a config file exists

## What is done

- Removed legacy shell-runtime dependence from the active repo contract.
- Implemented grouped Rust command families and kept large families split into thin module roots.
- Added `client plan` so routing, project-root resolution, server policy arbitration, and surface constraints are visible before live runtime exists.
- Added preview-only `client export` so supported client surfaces now have an explicit adapter contract, blocker list, and next-step guidance before config patching exists.
- Added bootstrap-only `stdio-shim --json` so the repo now has a real entrypoint for normalized session bootstrap and persistent-hub attach proof, without pretending that live MCP stdio forwarding already works.
- Added `lab list/matrix/coverage/gaps/report/show` so runtime claims are separated into covered / partial / blocked instead of being blurred together.
- Added top-level `repair` as a grouped maintenance shorthand over `hub repair` with native Rust integration coverage.
- Added a clean source archive builder and repo-contract tests for archive shape, doc drift, schema/example drift, stack drift, and fixture/capability parsing.
- Added project-control docs plus a more production-like prompt/agent eval governance set with machine-checked contracts.
- In this pass, restored missing planner helpers (`resolve_string`, `clean_optional_string`, `resolve_session_lease`) and the missing `CapabilityGap` import that would otherwise break Rust compilation.
- In this pass, tightened `doctor`/`verify readiness` so `runtime_prerequisites_ready` only turns green when required runtime prerequisites are actually present; today that means container-backed runtime servers imply Docker readiness.
- Added `scripts/proof-report.mjs` plus a repo contract test so the latest verification snapshot is generated from executed source/release checks and detected environment state.
- Synced package/manifests/reports/archive metadata to **0.3.0** and rebuilt the clean release archive.

## What is in progress

- Converting the current control plane into a real runtime core: `stdio` ingress, local Streamable HTTP ingress, exclusive lease enforcement, and cancel/stale-result guards.
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
3. Add lease ownership and cancel/stale-result guards.
4. Re-run build/runtime proof on supported hosts.
5. Only then finish `client install` and promote preview-only `client export` into real config patching.

## Key metrics

### Verified repo metrics

- source-level native command surfaces: **24** (`reports/rust-command-coverage.md`)
- grouped command families implemented now: **6** (`client`, `hub`, `init`, `lab`, `server`, `verify`)
- grouped commands still planned: **1** (`release`) plus the partial `client install` / config-writing `client export` surface
- runtime capability inventory: **24 total**
  - **13 implemented**
  - **11 planned**
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
2. **Coarse roadmap weighting**: roughly **45%–55% complete**.

Why the weighted view is a range rather than a single percentage:

- the already-finished work is biased toward **read-paths, planning, and source proof**
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
