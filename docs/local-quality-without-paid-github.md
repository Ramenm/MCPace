# Local quality without a paid GitHub plan

MCPace should be provable from a local checkout before any hosted workflow is trusted.
GitHub can mirror the checks, but it is not the source of truth for release decisions.

## The local-first rule

Use generated reports under `reports/` as the proof source:

```bash
npm run verify:toolbox
npm run verify:local:smoke
npm run verify:local:source
npm run verify:publish-decision
```

The source snapshot is acceptable only when `reports/publish-decision-latest.json` says:

```json
{
  "okForPublicSourceSnapshot": true
}
```

Native npm publication is acceptable only when the same report also says:

```json
{
  "okForNpmNativePublication": true
}
```

Today those are deliberately separate. Source can be ready while native runtime publication remains blocked by missing Rust/native/runtime proof.

## Profiles

| Profile | Command | Purpose |
|---|---|---|
| Smoke | `npm run verify:local:smoke` | Fast local bug/security/source sanity. |
| Source | `npm run verify:local:source` | Public source snapshot proof. |
| Full | `npm run verify:local:full` | Adds Rust quality, vendored binary, and runtime trace gates. |
| Release | `npm run verify:local:release` | Full profile plus pre-publish and publish-decision aggregation. |

## What does not require GitHub

These checks run without GitHub-hosted CI:

- Node syntax and Node contract tests;
- source audit;
- defect gates and bug sweep;
- high-confidence local secret scan;
- local supply-chain posture audit;
- Cargo metadata and Rust formatting;
- npm launcher/package dry-runs;
- platform package manifest checks;
- product-practice claim gate;
- publish decision report.

Full native runtime proof still needs a machine with Rust dependencies available and a staged native binary. That machine can be your workstation, a local server, a container host, or a self-managed runner.

## Recommended local tools

Core source proof works with Node/npm and Rust. For a serious release, also install:

```bash
cargo install cargo-audit --locked
cargo install cargo-deny --locked
cargo install cargo-nextest --locked
cargo install cargo-auditable --locked
```

Optional independent scanners:

```bash
# examples, install by your preferred package manager
osv-scanner --version
gitleaks version
trivy --version
```

Missing optional scanners produce warnings, not source blockers. They are release polish and supply-chain confidence signals.

## Fast bug-fix loop

For a bug fix:

1. Reproduce the bug with a minimal command, test, or runtime trace.
2. Add the smallest regression guard.
3. Patch the root cause, not just the symptom.
4. Run `npm run verify:local:smoke`.
5. Run the specific deeper gate for the touched area.
6. Regenerate `reports/publish-decision-latest.json` before sharing.

## No paid-plan assumption

The local reports are sufficient to decide whether the source tree can be made public. Hosted GitHub checks are useful trust signals once the repository is public, but the project should remain buildable, testable, and reviewable without depending on a paid account feature.
