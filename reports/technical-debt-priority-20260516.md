# Technical debt priority pass — 2026-05-16

This pass focuses on release-proof, eval-proof, and security-boundary debt visible in the source archive.

## Found debt by category

### 1. Archive hygiene / release evidence

Description: The previous release manifest bundled the whole `reports` directory. That mixed current proof, generated reports, historical reports, and stale older-version artifacts into the source archive.

Risk: Consumers can mistake historical `*-latest.json` files or old native-binary reports for current release proof. This undermines the repo's own product-truth discipline and release decision gates.

Effort: Low.

Priority: High.

Action taken: `release-manifest.json` now includes selected useful report files only: `summary.md`, `verification-latest.json`, `rust-command-coverage.json`, `tombstones.md`, and the new second-pass reports. `tests/node/archive-contract.test.js` now asserts that stale generated reports such as `publish-decision-latest.json` and `code-inventory-latest.json` are not bundled in release archives.

### 2. Version drift after source changes

Description: The source snapshot was still versioned as `0.6.0`. The maintainer's archive rule requires a patch bump for a changed artifact.

Risk: Reviewers cannot distinguish the previous patched archive from the second-pass archive; generated reports and package manifests become ambiguous.

Effort: Low.

Priority: High.

Action taken: Current manifests and eval/product-truth metadata were bumped to `0.6.2`. Historical docs and changelog entries for earlier versions were not rewritten.

### 3. Rust host proof remains missing

Description: `cargo fmt`, `cargo check`, `cargo test`, `cargo clippy`, and release build proof cannot run in the current sandbox because `cargo`/`rustc` are unavailable.

Risk: Source-level Node checks may pass while Rust compile errors, formatting errors, or clippy issues remain hidden. This must block release-ready claims.

Effort: Medium, because it requires a Rust host rather than more source editing.

Priority: High.

Recommended action: Run the Rust proof commands listed in `reports/summary.md` on a real Rust host, then regenerate `reports/rust-quality-latest.json`, `reports/verification-latest.json`, and publish-decision artifacts.

### 4. Local compat crates share upstream crate names

Description: `Cargo.toml` still points `auto-launch`, `getrandom`, `serde_json`, and `which` to local compat crates with names matching public crates.

Risk: Reviewers may assume upstream behavior even when local behavior differs. This is most sensitive for `getrandom`, although the insecure fallback has already been removed.

Effort: Medium to high, because renaming crates can touch imports, lockfile, docs, and tests.

Priority: Medium.

Recommended action: Keep the current fixed implementation for this patch, but open a follow-up to rename local shims to `mcpace_compat_*` or document a strict allowlist with per-crate rationale.

### 5. Eval assets exist, but execution runner is still governance-only

Description: The repo has scenario matrices, fixtures, and contract tests, but no dedicated script that executes prompt/model eval cases end-to-end against a model/tool loop.

Risk: Eval files can stay structurally correct while real prompt/model behavior regresses.

Effort: Medium.

Priority: Medium.

Recommended action: Add a small offline evaluator first for binary fixture checks, then add a provider-backed runner only after deciding cost/latency handling and secret boundaries.

### 6. Historical reports remain in the working tree

Description: Many old reports remain under `reports/` for project history.

Risk: They can be useful context locally, but can confuse readers if displayed as current proof.

Effort: Low to medium.

Priority: Medium after archive hygiene.

Recommended action: Keep them out of source archives, then either move historical proof to `reports/history/` with an index or add a `reports/README.md` that marks current vs historical evidence.

## Prioritization matrix

| Item | Risk | Effort | Priority | Current status |
|---|---:|---:|---:|---|
| Archive bundles stale/generated reports | High | Low | High | Fixed in this pass |
| Version drift after changes | Medium | Low | High | Fixed in this pass |
| Missing Rust host proof | High | Medium | High | Still blocked by environment |
| Local compat crates share upstream names | Medium | Medium/High | Medium | Follow-up recommended |
| Eval runner is structural, not model-executing | Medium | Medium | Medium | Follow-up recommended |
| Historical reports remain in working tree | Medium | Low/Medium | Medium | Mitigated by archive pruning |

## Recommended closure order

1. Keep archive pruning and version alignment.
2. Run Rust proof on a real Rust host.
3. Regenerate current proof reports only after Rust proof passes.
4. Add a `reports/README.md` or move historical reports behind a clear history boundary.
5. Decide whether to rename local compat crates.
6. Add an eval runner after the fixture governance is stable.

## Safe immediate fixes already done

- Archive manifest narrowed to selected report files.
- Archive regression test added.
- Version metadata bumped to `0.6.2`.
- Maintainer operating mode documented.
