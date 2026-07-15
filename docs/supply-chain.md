# Supply-chain policy

This repository is intentionally configured to fail closed for release and CI supply-chain checks.

## npm installs

Project-local `.npmrc` sets `ignore-scripts=true` so third-party lifecycle scripts are disabled by default during local and CI installs. CI commands also pass `--ignore-scripts` explicitly for defense in depth.

## Dependency lockfile policy

`npm run check:dependency-policy` verifies that:

- the npm lockfile is lockfileVersion 3;
- external locked packages have integrity metadata;
- external locked packages resolve from the npm public registry only;
- locked packages do not declare install lifecycle scripts;
- the native optional binary packages are exact-version `@mcpace/cli-*` packages matching the launcher package version;
- Cargo standard-crate dependencies are not redirected to local `crates/compat` shims.

### Cargo lock refresh warning

When the Rust toolchain is unavailable, the dependency policy can only warn that `Cargo.lock` needs refresh after a `Cargo.toml` dependency change. Treat `cargo-lock-standard-crates-synced` warnings as a release blocker. Update only the intended package (for example, `cargo update -p <crate> --precise <version>`), review the complete lockfile diff, and then run all locked check, test, Clippy, and release-build gates. Validation commands must not rewrite the lockfile.

## Workflow policy

`npm run check:workflow-policy` verifies the local GitHub Actions policy:

- workflows declare explicit permissions;
- publish uses npm trusted publishing shape: OIDC permission, stable `vX.Y.Z` tags, unique `dev` prereleases, a protected environment, no long-lived npm token fallback, immutable release-SHA metadata, and an enforced native package contract;
- release artifacts have GitHub artifact attestation permissions and an attestation step;
- inline shell blocks do not interpolate untrusted GitHub expressions directly;
- third-party actions are at least explicitly ref-pinned. Tag-pinned actions are warnings by default and become failures with `--enforce-sha`.

Package-side npm trusted publisher setup is automated with `npm run npm:trust:configure`. This command still requires an authenticated npm owner session with 2FA, but it avoids manual per-package clicks by running `npm trust github` for the main package and every enabled native package.

## Source evidence report

`npm run check:supply-chain-evidence` is a report-only command. It emits `mcpace.supplyChainEvidence.v1` and may exit successfully while reporting `status: "blocked"`; callers must inspect `status` and `blockers`. The report checks npm lockfile integrity (including lockfile-v3 `hasInstallScript` metadata), disabled lifecycle scripts, Cargo dependency/lockfile evidence, release-manifest hygiene, and workflow shape for OIDC, provenance, artifact attestations, CodeQL, and OpenSSF Scorecard.

`npm run check:endgame:enforce` is the fail-closed aggregate release gate. `npm run evidence:supply-chain` writes the current report to `reports/supply-chain-evidence.json` for release bundles and audits.
