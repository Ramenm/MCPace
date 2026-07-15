# MCPace legacy retirement plan

MCPace should not treat `legacy` as a normal feature bucket. Legacy code is allowed only when it is one of these explicit states:

1. **Retired** — old files, flags, wrappers, or registry entries are removed and guarded by tests.
2. **Cleanup-only** — code exists only to delete an old user/system artifact, for example the retired `MCPace` autostart entry.
3. **Quarantined compatibility** — the input is still recognized for migration, but runtime behavior is disabled, plan-only, or routed through an explicit adapter.
4. **Migration alias** — a public alias remains temporarily so existing client configs do not break, but the new command is documented as canonical.

Anything else is a bug. New legacy markers must either be rejected by `scripts/legacy-boundary-guard.mjs --enforce` or added to the allowlist with a retirement reason.

## Current quarantine zones

| Zone | Current files | Correct behavior | Retirement direction |
| --- | --- | --- | --- |
| Windows autostart cleanup | `src/service/legacy.rs`, coordinated from `src/service.rs`, tested by `src/service/tests.rs` | Remove old `MCPace` Run entry and keep only `MCPace Agent` using the hidden launcher plan. | Keep cleanup-only code quarantined in `src/service/legacy.rs`; delete it after a documented grace period and after support docs show no old entry remains. |
| Legacy SSE transport aliases | `src/source_type.rs`, `src/server/loader.rs`, `src/upstream.rs`, `src/client/plan.rs`, `src/hub/leases.rs` | Accept old names such as `sse`, `remote-sse`, and `sse-legacy`, normalize them, block direct forwarding, and keep them out of automatic parallel routing. | Move all literals behind a `transport`/`legacy_transport` boundary, then require explicit adapter config. |
| `stdio-shim` alias | `src/app.rs`, `src/stdio_shim.rs`, client import/export paths | Keep `mcpace stdio` as canonical; retain `stdio-shim` only for existing client configs. The exact preview/apply/restore migration is documented in `docs/supported-clients.md`. | Stop writing aliases in new configs now; announce deprecation before removal, and remove the command alias no earlier than `1.0.0`. |
| Versioned schema compatibility values | `schemas/*.json` | Preserve old enum values only because config/profile schemas are versioned contracts. | Introduce v2 schema names that describe behavior (`blocked-transport`, `adapter-required`) rather than `legacy`. |
| Release ZIP writer | `scripts/lib/zip-writer.mjs` | Keep contract tests while no maintained packager is wired. | Replace with a maintained ZIP implementation or cargo-dist/cargo-packager flow. |
| Raw HTTP/TCP | `src/http_probe.rs`, `src/dashboard*.rs` | Keep bounded by timeouts/limits and security tests. | Move outbound HTTP to a maintained client first; migrate dashboard server later under route/security contract tests. |

## Large-file split sequence

Do not split by moving random functions. Split by ownership boundary and preserve behavior with tests first.

1. Extract test modules from production files into `module/tests.rs` so production files are easier to read.
2. Split pure model/types before side-effecting code.
3. Split platform-specific code before generic code.
4. Split validation/classification from IO.
5. After each split, run `cargo test --locked -- --test-threads=1` and `node scripts/architecture-debt-inventory.mjs --json`.

Suggested target layout:

```text
src/service.rs
src/service/cli.rs
src/service/config.rs
src/service/legacy.rs
src/service/autostart_plan.rs
src/service/platform.rs
src/service/verify.rs
src/service/report.rs
src/service/tests.rs

src/server/loader.rs
src/server/loader/schema.rs
src/server/loader/sources.rs
src/server/loader/normalize.rs
src/server/loader/classify.rs
src/server/loader/validate.rs
src/server/loader/tests.rs

src/dashboard/overview.rs
src/dashboard/overview/model.rs
src/dashboard/overview/collect.rs
src/dashboard/overview/render.rs
src/dashboard/overview/access_review.rs
```

## Phase 2 status

The second cleanup wave has now made these boundaries explicit:

- inline Rust test bodies are no longer allowed in production modules; use `#[cfg(test)] mod tests;` plus `module/tests.rs`;
- `src/service.rs` is no longer allowed to grow past the monolith threshold while autostart code is being split;
- cleanup-only Windows autostart compatibility code lives in `src/service/legacy.rs`;
- `scripts/architecture-boundary-guard.mjs --enforce` prevents these boundaries from silently regressing.

The remaining large files should be split with move-only PRs first, then semantic refactors. Do not mix legacy deletion, module movement, and behavior changes in the same PR unless Rust and Node checks are green on every platform.

## Guardrails

Run these during legacy cleanup:

```bash
node scripts/architecture-debt-inventory.mjs --json
node scripts/architecture-boundary-guard.mjs --json --enforce
node scripts/legacy-boundary-guard.mjs --json --enforce
node scripts/legacy-subsystem-map.mjs --json
node scripts/modernization-inventory.mjs --json
```

A good cleanup PR should reduce at least one of:

- large production Rust file count;
- public root module count in `src/lib.rs`;
- scattered `legacy-*`/`compat` string literals;
- raw HTTP/TCP inventory;
- stringly error inventory.
