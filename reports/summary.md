# MCPace v0.4.1 summary

## 1. Current task and real problem

The previous MCP hardcode removal was mostly solved: packaged upstream MCP defaults remain empty and user-supplied stdio MCP servers are configured through `mcp_settings.json`. The next production-relevant risk was not another catalog default; it was that diagnostics and CI could still regress the safety/reliability story:

- upstream stderr is useful for diagnosing arbitrary MCP servers, but raw stderr can contain tokens, Authorization headers, or secret-bearing command output;
- Rust CI lanes repeatedly resolve/build dependencies and had no Cargo cache despite a checked-in `Cargo.lock`;
- checkout steps persisted the GitHub token even in jobs that only read source;
- full Rust proof is still environment-dependent and must not be overstated from this sandbox.

Reframed task: preserve generic source-only MCP support while reducing secret-leak, CI flakiness/cost, and false proof claims.

## 2. Source map checked

### Repo sources

- `README.md`, `docs/README.md`, `docs/test-strategy.md`.
- `memory-bank/*.md`.
- `mcp_settings.json`, `mcpace.config.json`, `server-candidates.json`.
- `src/upstream.rs` for stdio upstream launch, stderr handling, env parsing, cache fingerprints.
- `.github/workflows/{ci,release-dry-run,release,publish-npm}.yml` for CI/CD.
- `package.json`, `.nvmrc`, `.node-version`, `rust-toolchain.toml`, `Cargo.toml`, `Cargo.lock`.
- `tests/node/*`, Rust tests embedded in `src/upstream.rs`.
- `eval/*` and seed/runtime fixtures.

### External sources

- Model Context Protocol transport spec: https://modelcontextprotocol.io/specification/2025-03-26/basic/transports — stdio subprocess, JSON-RPC over stdin/stdout, stderr logging behavior.
- OpenAI Codex MCP docs: https://developers.openai.com/codex/mcp — stdio MCP config fields such as command/args/env.
- GitHub dependency caching reference: https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching — cache key/restore-key behavior and sensitive-data warning.
- GitHub Rust build/test docs: https://docs.github.com/en/actions/tutorials/build-and-test-code/rust — official Cargo cache shape for `~/.cargo/registry`, `~/.cargo/git`, and `target` keyed by `Cargo.lock`.
- GitHub `actions/setup-node` docs: https://github.com/actions/setup-node — npm cache depends on lockfiles and does not cache `node_modules`.
- GitHub `actions/checkout` docs: https://github.com/actions/checkout — checkout persists auth token by default; `persist-credentials: false` opts out.
- OWASP Logging Cheat Sheet: https://cheatsheetseries.owasp.org/cheatsheets/Logging_Cheat_Sheet.html — logs/diagnostics are valuable but must be handled as security-relevant data.
- Cargo Book CI guide: https://doc.rust-lang.org/cargo/guide/continuous-integration.html — deterministic CI and dependency update trade-offs.

## 3. Independent tracks

### Track A — Existing project pattern

How it works here: MCPace already uses proof-first repo contracts, Node `node:test` checks for docs/manifests/workflows, Rust unit tests for runtime behavior, ADRs for architectural decisions, and memory-bank files for durable context.

Pros: low regression risk, follows current scripts, no new dependency stack. Cons: large modules such as `src/upstream.rs` remain hard to review. Best when the goal is safe incremental hardening without changing runtime contract.

### Track B — Classic industry approach

How it works here: keep logs useful but sanitize secrets, bound untrusted diagnostic output, cache deterministic build dependencies, avoid caching secret-bearing paths, and minimize CI token exposure.

Pros: aligns with OWASP/GitHub/Cargo guidance, improves reliability and AppSec without product behavior changes. Cons: heuristic redaction is not a full DLP system; Cargo target caches require monitoring. Best when build/runtime reliability and safety are the highest-value next step.

### Track C — Alternative approaches

Options: suppress all stderr, forward raw stderr for maximum debugging, use a third-party Rust cache action, or implement a structured telemetry layer before any patch.

Pros: raw stderr is simplest for debugging; a third-party cache action may be more ergonomic; structured telemetry is best long-term. Cons: raw stderr leaks secrets, suppressing all stderr hurts support, third-party actions add dependency risk, and full telemetry is larger scope. Best when production incident data proves the heuristic is insufficient or when a real telemetry backend exists.

Synthesis: tracks converge on a clear answer for this pass — keep source-only MCP behavior, sanitize bounded stderr, harden CI with official Cargo caching and checkout credential minimization. The core decision is obvious; the main trade-off is heuristic redaction vs diagnostic detail.

## 4. Implementation variants considered

| Variant | внедрение | поддержка | риски | производительность | совместимость | лицензия / стоимость |
|---|---|---|---|---|---|---|
| A. Incremental stderr sanitizer + CI Cargo cache | Low/medium | High: current contracts/ADRs | Redaction false negatives/positives | Better CI warm runs | No MCP config change | No new deps/cost |
| B. Suppress all upstream stderr | Low | Simple | Poor diagnosability | Neutral | Compatible but opaque | No new deps/cost |
| C. Raw stderr passthrough | Low | Simple | High secret leakage | Neutral | Best debugging | No new deps/cost |
| D. Full structured telemetry module | High | Best long-term | Larger regression/scope | Potentially better diagnosis | Needs rollout | No immediate deps, higher effort |

Chosen: Variant A. It improves security, CI reliability, and observability with minimal behavior change.

## 5. Changes made

### Security / diagnostics

- Added bounded upstream stderr sanitization in `src/upstream.rs` before stderr is appended to user-visible upstream errors.
- Redacts likely token/password/passwd/secret/API-key/access-key/private-key/credential/Authorization assignments.
- Redacts `Bearer <token>` values.
- Preserves safe context and bounds stderr diagnostics by line count and per-line length.
- Added Rust unit tests:
  - `stderr_suffix_redacts_sensitive_diagnostics_without_removing_context`;
  - `stderr_suffix_bounds_diagnostic_line_count_and_length`.
- Added Node contract test `tests/node/security-contract.test.js` to keep sanitizer wiring and docs visible in source proof.

### CI/CD

- Added `actions/cache@v4` to Rust quality, Rust lifecycle, hosted launcher smoke, release dry-run native, and release native jobs.
- Cache keys include runner OS, Rust `1.95.0`, target or suite where relevant, `Cargo.lock`, and `rust-toolchain.toml`.
- Added restore keys for safe partial Cargo cache reuse.
- Added `persist-credentials: false` to checkout steps that do not need persistent git auth.
- Updated repo/release workflow contract tests to guard Cargo cache wiring and checkout credential minimization.

### Eval / docs / memory

- Added adversarial eval fixture `eval/fixtures/seed/mcp-stderr-secret-leak-regression.json`.
- Updated `eval/scenario-matrix.json` and `eval/dataset-plan.json`.
- Added ADR `docs/adr/0005-ci-cache-and-upstream-diagnostic-redaction.md`.
- Updated `README.md`, `docs/README.md`, `docs/test-strategy.md`.
- Updated `memory-bank/*` with current state.
- Bumped project version from `0.4.0` to `0.4.1` across manifests/docs/reports.

## 6. Security review findings

| Severity | Problem | Where | Risk | Recommendation / status |
|---|---|---|---|---|
| High | Raw upstream stderr could include secrets from arbitrary user-supplied MCP servers | `src/upstream.rs::stderr_suffix` | Token/API-key/Authorization leakage into errors, logs, support output | Fixed with bounded redaction and tests. |
| Medium | Checkout persisted GitHub token in read-only jobs | `.github/workflows/*.yml` | Accidental token availability to later shell steps | Fixed with `persist-credentials: false` on checkout steps. |
| Medium | Rust CI had no Cargo cache | Rust CI/release jobs | Slower and more network-sensitive CI | Fixed with official `actions/cache@v4` Cargo caches. |
| Medium | npm cache was tempting but no npm lockfile exists | package/workflow state | Caching without lockfile would be weaker and potentially fragile | Not added; document that npm cache should wait for a lockfile. |
| Low | Redaction is heuristic | `src/upstream.rs` | Unusual secret formats can evade sanitizer | Keep bounded output; consider structured telemetry/redaction module later. |

## 7. Integration / regression tests

Dependencies and fixtures:

- Node tests use built-in `node:test`; no external packages.
- Rust tests are unit-level and use in-memory channels for stderr sanitizer behavior.
- Eval fixture is JSON-only and validated by existing eval contract tests.

Covered scenarios:

- Main flow: source-only MCP and CI source validation still run through `npm test`.
- Negative/degraded flow: secret-bearing stderr is redacted instead of copied raw.
- Additional CI case: Rust workflows keep deterministic Cargo caches and checkout credential minimization.

PASS criteria:

- `npm test` passes.
- `tests/node/security-contract.test.js`, repo workflow contracts, and eval contract pass.
- `cargo fmt --all -- --check` passes under available stable toolchain.
- Full Rust quality gate still requires pinned Rust 1.95.0 and network/cache access.

## 8. Eval goals and matrix update

Eval goals:

- Catch agent/prompt changes that improve debugging by leaking secrets.
- Penalize raw Authorization/bearer/token output in diagnostics.
- Reward preserving safe diagnostic context instead of hiding all stderr.

Scenario matrix:

- Family: `mcp-configuration-safety`.
- Added adversarial seed: `mcp-stderr-secret-leak-regression`.

Rubric/metrics:

- Existing dimensions remain: task success, factual support, honesty/uncertainty, scope control, actionability.
- Metrics remain: task-success rate, unsupported-claim rate, uncertainty rate.
- Binary checks now include must-preserve safe diagnostics and must-not-leak secret diagnostics.

Main failure modes now caught:

- Raw upstream stderr echoed into errors.
- Unbounded stderr output.
- Fixes that remove all stderr context and make startup failures opaque.

## 9. CI/CD review

Where it was brittle:

- Rust jobs depended on cold dependency/build state every run.
- Checkout token persisted despite mostly read-only jobs.
- npm cache would be attractive but unsupported by current repo shape because no npm lockfile is checked in.

Point changes:

- Official Cargo caches in Rust-heavy jobs.
- Credential persistence disabled on checkout.
- Contract tests assert cache/credential posture.

What becomes more stable:

- Warm CI runs should spend less time resolving/building unchanged Rust dependencies.
- Read-only jobs expose fewer credentials to shell steps.
- Release dry-run and release workflows now match CI cache posture.

How to verify:

```bash
npm test
node --test --test-reporter spec tests/node/repo-contract.test.js tests/node/release-workflow-contract.test.js tests/node/security-contract.test.js
```

Then in GitHub Actions, verify first-run cache miss followed by later cache hit for Rust jobs.

## 10. Observability / performance notes

Blind spot addressed: upstream stderr was either useful but risky, or could become fully suppressed. The new behavior keeps sanitized, bounded context.

Performance bottleneck addressed: Rust CI dependency/build reuse. Baseline is observable through GitHub Actions cache hit/miss and job duration before/after this change. Expected effect: lower warm-run Rust job duration and lower network sensitivity; exact savings are NOT CONFIRMED until CI history is available.

## 11. Technical debt priority

| Category | Description | Risk if left | Effort | Priority |
|---|---|---|---|---|
| Large module | `src/upstream.rs` remains large and now owns env, launch, pooling, redaction, and cache logic | Harder review and future redaction/pooling changes | Medium/high | High |
| Full Rust proof blocked locally | Sandbox cannot resolve pinned Rust/deps | Compile/test regressions can only be ruled out in CI | Low in supported CI | High |
| Redaction heuristic | No formal DLP/parser module | Secret formats outside heuristic can pass | Medium | Medium |
| No npm lockfile | npm cache intentionally not enabled | Less deterministic npm tooling if dependencies are added later | Medium | Medium |
| HTTP upstream fan-out | Still inventory/diagnostic-only | Product overclaim if docs drift | High | Medium |

Recommended order:

1. Run full Rust quality gate in supported CI.
2. Extract upstream diagnostics/redaction into a smaller module after green Rust proof.
3. Add a real stdio MCP fixture integration test.
4. Decide whether to add npm lockfile before enabling npm cache.
5. Keep HTTP fan-out as a separate transport adapter decision.

## 12. API/spec check

No OpenAPI, GraphQL schema, proto, or RAML spec was found in the project. The relevant external protocol is MCP JSON-RPC over stdio/HTTP. This pass does not introduce a new API operation and does not change the public wrapper-tool contract. It changes error diagnostic sanitization and CI workflow behavior.

Breaking changes: none intended for MCP config. Operational change: error messages may redact sensitive-looking stderr values and truncate long stderr lines.

## 13. Known / assumed / unknown

Known:

- Packaged MCP defaults remain empty.
- Stdio child env isolation from v0.4.0 remains in place.
- Cwd-aware stdio command resolution/validation is present for user-supplied MCP servers.
- This pass adds stderr sanitizer, Cargo CI cache, checkout credential minimization, npm publish exact npm exec, eval fixture, ADR, docs, and contract tests.

Assumed:

- CI cache paths are acceptable for this repo's current dependency set.
- Redacting common key/value and bearer patterns is better than raw stderr passthrough.

Unknown / NOT CONFIRMED:

- Exact CI speedup; needs GitHub Actions history.
- Whether any real MCP server emits secrets in nonstandard stderr formats that evade the heuristic.
- Whether future npm dependencies justify checking in a lockfile and enabling npm cache.

## 14. Actual verification in this sandbox, 2026-05-01

Environment observed for this pass:

- Linux x86_64, `/bin/bash`, root container.
- Runtime available locally: Node `v18.19.0`, npm `9.2.0`, stable Rust `cargo 1.85.0` / `rustc 1.85.0`.
- Project-declared runtime remains stricter: Node `24` in `.nvmrc` / `.node-version`, package engines Node `>=22`, npm `>=10`, and Rust `1.95.0` in `rust-toolchain.toml`.

Commands that passed here:

```bash
node --test --test-reporter spec   tests/node/security-contract.test.js   tests/node/repo-contract.test.js   tests/node/release-workflow-contract.test.js   tests/node/mcp-config-contract.test.js   tests/node/eval-contract.test.js   tests/node/publish-npm-artifacts-contract.test.js   tests/node/product-truth-contract.test.js
# PASS: 25 tests

node --test --test-reporter spec packages/npm/cli/test/*.test.mjs
# PASS: 13 tests

node scripts/audit-source.mjs --json --fail-on-critical --write reports/source-audit-latest.json
# PASS: ok=true, critical=[]

MCPACE_NPM_PACK_TIMEOUT_MS=10000 node scripts/verify-npm-pack.mjs --json
# PASS: @mcpace/cli thin-launcher package includes required files, version 0.4.1

node scripts/verify-publish-readiness.mjs --json
# PASS/PENDING: workflow has no issues; repository URL remains absent in this archive, so strict trusted-publishing metadata is pending until repo context exists.

RUSTUP_TOOLCHAIN=stable cargo fmt --all -- --check
# PASS
```

Checks attempted but not fully proven here:

- `npm test` under local Node 18/npm 9 timed out during the long npm-driven source lane. This sandbox does not match the project-declared Node/npm versions, so this is recorded as an environment limitation rather than a confirmed project failure.
- `cargo test` / `cargo clippy` / `cargo check` cannot be completed here because the pinned Rust `1.95.0` toolchain and crates.io dependency resolution require network/cache access that this sandbox does not have.

Recommended supported-environment verification:

```bash
npm test
npm run verify:rust-quality
RUSTUP_TOOLCHAIN=1.95.0 cargo check --all-targets --locked
RUSTUP_TOOLCHAIN=1.95.0 cargo test --all-targets --locked
RUSTUP_TOOLCHAIN=1.95.0 cargo build --release --locked
```

## 15. Self-check questions and answers

- Does this reintroduce hardcoded MCP servers? No; config/catalog defaults stay empty.
- Does this change MCP command/env config compatibility? No intended config-shape change.
- Does this hide all diagnostics? No; it preserves safe context and bounds/redacts risky values.
- Did I claim full Rust proof from this sandbox? No; only targeted Node contracts, npm launcher tests, source audit, npm pack verification, publish readiness workflow validation, and Rust fmt are locally proven here.
- Is there a safer bigger rewrite? A diagnostics module would be cleaner, but higher risk before full Rust proof.

## 16. Backlog / ETA recalculation

Scope added in this pass:

- stderr sanitizer and tests;
- CI Cargo cache and checkout hardening;
- npm publish exact npm exec hardening;
- eval regression fixture;
- ADR/docs/memory updates;
- packaging/version bump.

Scope not added:

- HTTP upstream fan-out;
- full diagnostics/telemetry module;
- npm lockfile/cache;
- upstream module split.

Effort estimate for remaining recommended backlog: medium. ETA is NOT CONFIRMED because CI runtime history and team capacity are unavailable.
