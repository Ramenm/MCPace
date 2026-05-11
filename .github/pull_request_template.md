## Why

Describe the intent of the change and the constraint it answers.

Linked issue:

## What Changed

-

## Bug / Regression Closure

- [ ] This is not a bugfix/regression fix
- [ ] Reproduction or failing test was captured before the fix
- [ ] Root cause is described below
- [ ] Regression test or proof artifact now protects the behavior

Root cause:

Regression test:

Runtime trace:

## Verification

- [ ] `npm test`
- [ ] `npm run verify:defect-gates`
- [ ] `npm run verify:bug-sweep`
- [ ] `npm run verify:github-readiness`
- [ ] `npm run verify:product-practice`
- [ ] `npm run pack:npm:dry-run`
- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --all-targets --locked -- -D warnings` (when a Rust host is available)
- [ ] `cargo test --all-targets --locked` (when a Rust host is available)
- [ ] Additional targeted verification:

## Repo Contract Hygiene

- [ ] No deleted PowerShell entrypoints were reintroduced
- [ ] Docs/tests/manifests/reports stay aligned with the actual repo state
- [ ] Unsupported or unproven behavior is described honestly
- [ ] New public claims have fresh proof reports

## Risks

-

## Not-tested

-
