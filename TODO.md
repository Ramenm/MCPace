# TODO

## Backlog assumptions

- Estimates are **coarse point ranges**, not historical throughput.
- A point is only a relative sizing unit. In this repo it roughly means **one focused half-day to two days** depending on host/tooling availability.
- ETA ranges assume **one focused maintainer**, a working Rust host, and **no major scope growth**.
- Anything that needs real client traces, Windows/macOS runs, or published-package proof stays **blocked** until those environments exist.

## Status summary

- done baseline: planner/read-path surface, hub lifecycle shell, runtime lab, archive builder, and Node/source proof loop
- in progress: moving from read-path honesty to live runtime correctness
- blocked: real host/runtime proof, real-client compatibility traces, published release proof
- honest completion view: roughly **45%–55%** of the currently known roadmap; the repo is further along on **control plane + source proof** than on **live runtime + release proof**

## Recently done baseline

| ID | Status | Work | Evidence |
|---|---|---|---|
| D1 | done | Rust-native grouped read surface for `client`, `hub`, `lab`, `server`, `verify`, plus `init`, `doctor`, `version`, `candidates` | `reports/rust-command-coverage.md`, `src/` |
| D2 | done | Local file-backed hub lifecycle shell: `hub up/down/status/logs/repair` | `src/hub/`, `tests/hub_runtime.rs` |
| D3 | done | Surface-aware client catalog and routing planner | `src/client.rs`, `src/client_catalog.rs`, `docs/client-surface-matrix.md` |
| D4 | done | Runtime lab with production-like fixtures and capability-gap reporting | `src/lab.rs`, `eval/runtime-capabilities.json`, `eval/fixtures/runtime/` |
| D5 | done | Clean source archive builder and repo/source contract tests | `scripts/archive-release.mjs`, `tests/node/*.test.js` |
| D6 | done | Project-control docs and eval-governance contracts added in this pass | `TODO.md`, `STATE.md`, `DECISIONS.md`, `eval/*.json` |
| D7 | done | Fixed source-level planner/readiness regressions: restored missing `client/context` helpers, restored `lab` gap import, and tied readiness to real runtime prerequisites instead of config presence alone | `src/client/context.rs`, `src/lab/render.rs`, `src/doctor.rs` |
| D8 | done | Synced manifests/reports/archive metadata to `0.3.0` and rebuilt the clean release ZIP contract | `Cargo.toml`, `package.json`, `packages/npm/cli/package.json`, `reports/verification-latest.json`, `dist/` |
| D9 | done | Automated the verification snapshot so `reports/verification-latest.json` is generated from executed source/release checks instead of hand-edited status | `scripts/proof-report.mjs`, `tests/node/proof-report.test.js`, `reports/verification-latest.json` |
| D10 | done | Added grouped top-level `repair` as a native shorthand over `hub repair` so recovery stays on the public Rust CLI surface | `src/repair.rs`, `src/app.rs`, `tests/help_and_root.rs` |

## Prioritized open backlog

| Pri | ID | Status | Work | Points | Depends on | Definition of done | Risks | ETA range |
|---|---|---|---|---:|---|---|---|---|
| P0 | R1 | in progress | Promote bootstrap-only `mcpace stdio-shim` into a live stdio ingress that normalizes initialize metadata and forwards into the persistent hub | 13–21 | planner context resolution, hub lifecycle shell | `stdio` launcher forwards live MCP traffic; `_meta`/cwd/roots normalization matches planner; cancellation path tested; no overclaim in docs | host-specific stdio quirks; session stickiness bugs | 1–3 weeks after Rust host is available |
| P0 | R2 | not started | Ship local Streamable HTTP ingress with localhost-only defaults and MCP session handling | 13–21 | R1 optional, hub lifecycle shell | local HTTP endpoint exists; origin/binding rules enforced; session create/reuse/close covered; readiness docs updated | transport drift from spec baseline; auth/session edge cases | 2–4 weeks |
| P0 | R3 | not started | Enforce exclusive leases for `single-session` / `shared-exclusive` servers | 8–13 | R1 or R2, current planner warnings | owner tracking, heartbeat/timeout, takeover rules, clear errors, regression coverage | stale ownership, deadlocks, restart races | 1–2 weeks |
| P0 | R4 | not started | Add cancel/restart guards so dead leases cannot accept stale results | 8–13 | R3, ingress work | in-flight registry exists; cancel propagates; restart drops stale responses; adversarial lab cases can move from blocked to partial/covered | subtle race conditions and transport mismatch | 1–2 weeks |
| P1 | A1 | in progress | Implement `client install` / `client export` as manifest-driven config patchers | 8–13 | current client catalog, owned-block patch rules | at least one local surface and one cloud/API surface can install/export; dry-run and diff output exist; docs honest about supported targets | config shape drift across clients | 2–4 weeks |
| P1 | A2 | not started | Prove config-merge safety and owned-block patching against real-looking client configs | 5–8 | A1 | fixtures include mixed user-managed config; patcher preserves non-MCPace blocks; rollback path exists | destructive config writes; incomplete client samples | 1–2 weeks |
| P1 | M1 | not started | Finish grouped top-level `release` command | 3–5 | archive builder | command exists, is documented honestly, and only claims source/build proof that it can actually run | pressure to overclaim publish readiness | 2–5 days |
| P1 | Q1 | not started | Harden custom JSON parser with focused correctness tests | 3–5 | none | tests cover surrogate pairs, leading-zero rejection, malformed numbers/escapes, and non-ASCII strings | parser bugs remain hidden until runtime inputs get wider | 2–5 days |
| P1 | Q2 | not started | Expand `doctor`/`verify readiness` prerequisite mapping beyond Docker as more runtime kinds appear | 3–5 | R1/R2 design stability | each runtime kind declares its prerequisites; readiness output explains why a lane is blocked; docs/tests stay aligned | too-early generalization before ingress contracts settle | 2–5 days after ingress contracts stabilize |
| P1 | B1 | blocked | Re-run build proof on a host with `cargo`, `rustc`, and supported OS lanes | 8–13 | Rust toolchain, CI hosts | `cargo test`, `cargo build --release`, and later `cargo fmt --check` / `cargo clippy -- -D warnings` pass on supported hosts | not possible in current container; host drift | 1–2 weeks once hosts exist |
| P1 | B2 | blocked | Re-run runtime proof on Ubuntu/macOS/Windows with real prerequisites | 8–13 | R1–R4 at least partially done, supported hosts | `verify doctor`, `verify readiness`, and live ingress smoke pass on real hosts; results written to verification report | Docker/client availability, platform-specific behavior | 2–4 weeks after runtime core lands |
| P1 | E1 | in progress | Keep prompt/agent evals production-like, split by typical/edge/adversarial/held-out, and tied to real regressions | 5–8 | current eval docs and fixtures | every fixture has grounding, binary checks, rubric dimensions, and held-out handling; regression loop is written down and machine-checked | easy to slip into showroom evals without real traffic/traces | 3–6 days for baseline; ongoing refresh afterwards |
| P1 | E2 | blocked | Add sanitized historical traces and real-host cases to eval datasets | 5–8 | B2, safe trace capture process | held-out set includes sanitized real failures; no secrets/PII; source of each case documented | access to logs, privacy review, inconsistent trace quality | 1–3 weeks after traces exist |
| P2 | C1 | blocked | Build a real-client compatibility matrix for Codex, Claude, Cursor, Kiro, Windsurf, Copilot, Gemini, Hermes | 8–13 | B2, real client access | initialize/session/roots/auth traces captured; compatibility notes updated; at least one held-out case per major surface | closed/cloud surfaces may be inaccessible in CI | 2–5 weeks |
| P2 | P1 | not started | Add `profile` mutations and `projects` scan/write flows only after runtime core is stable | 5–8 | runtime core maturity | write paths exist with tests and docs; no silent mutation of user state | low immediate product value vs runtime core | 1–2 weeks |

## Ordering notes

1. The next best step remains **R1 (`stdio-shim`)** because it unlocks real runtime proof instead of only expanding documentation.
2. **A1/A2** should wait until the ingress/lease contract is stable enough that client onboarding is not built on a moving target.
3. **B1/B2/C1/E2** are real blockers, not paper tasks. They depend on environments and traces that are not present in this container.
4. **Q2** matters because the latest readiness fix intentionally handles only the container-backed runtime slice that is already visible in repo fixtures/configs.
