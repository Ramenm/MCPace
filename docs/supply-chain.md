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
- the native optional binary packages are exact-version `@mcpace/cli-*` packages matching the launcher package version.

## Workflow policy

`npm run check:workflow-policy` verifies the local GitHub Actions policy:

- workflows declare explicit permissions;
- publish uses npm trusted publishing shape: OIDC permission, planned branch channels (`main`/`master` stable, `dev` prerelease), protected environment, no long-lived npm token env fallback, and enforced native package contract;
- release artifacts have GitHub artifact attestation permissions and an attestation step;
- inline shell blocks do not interpolate untrusted GitHub expressions directly;
- third-party actions are at least explicitly ref-pinned. Tag-pinned actions are warnings by default and become failures with `--enforce-sha`.

Package-side npm trusted publisher setup is automated with `npm run npm:trust:configure`. This command still requires an authenticated npm owner session with 2FA, but it avoids manual per-package clicks by running `npm trust github` for the main package and every enabled native package.
