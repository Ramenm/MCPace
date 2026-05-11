# Offline quality and publish gates

MCPace should be provable on a local machine before it relies on any hosted CI.
GitHub Actions and security features are useful for a public repository, but they are not the source of truth for this project. The source of truth is a fresh local proof report under `reports/`.


## Local-first source decision

For a quick yes/no answer that does not require a paid GitHub plan, run:

```bash
npm run prove:local-first
npm run verify:publish-decision
```

The source tree can be shared publicly only when `reports/publish-decision-latest.json` has `okForPublicSourceSnapshot: true`. Native npm/runtime publication remains separately blocked until Rust quality, vendored binary, runtime trace, and product-practice release gates are all fresh and green.

Read `docs/local-quality-without-paid-github.md` and `docs/release-decision-runbook.md` for the exact policy.

## Fast local loop

Use this while changing code:

```bash
npm run verify:tooling
npm run verify:local-prepublish:quick
```

`verify:tooling` checks whether the local machine has the tools needed to prove the project: Node/npm, Cargo/rustc/rustfmt/Clippy, and recommended security/release tools such as `cargo-audit`, `cargo-deny`, `cargo-nextest`, and `cargo-auditable`.

`verify:local-prepublish:quick` is the fast hygiene gate. It runs source syntax checks, source audit, defect gates, bug sweep, public-repository docs/readiness checks, npm package shape checks, Cargo metadata, and Rust formatting. It deliberately does not imply runtime beta readiness.

## Full local pre-publish gate

Before any public release or stronger README claim, run:

```bash
npm run verify:local-prepublish
```

This is intentionally stricter than the fast loop. It requires the quick lane plus:

- Rust quality proof: format, Clippy, Rust tests, release build;
- host-compatible vendored native binary proof from `npm run verify:vendored-binary`;
- runtime trace: client -> `/mcp` -> initialize -> tools/list -> upstream tool call;
- install-readiness proof;
- product-practice claim gate.

The result is written to:

```text
reports/local-prepublish-latest.json
reports/local-prepublish-latest.md
```

If the report status is `blocked`, do not publish. Fix the first blocker, rerun the gate, and keep the updated report.

## Decision table

| Report status | Meaning | Publish? |
|---|---|---:|
| `pass` | Required local proof is fresh and complete. | yes, after final human review |
| `pass-with-warnings` | No required blocker, but polish/security/tooling warnings remain. | not for a polished launch |
| `blocked` | Required proof is missing or failed. | no |
| `planned` | No commands were run. | no |


## Optional local Git hook

For a personal checkout, install a local pre-push hook that runs the quick gate before code leaves your machine:

```bash
npm run hooks:install:dry-run
npm run hooks:install
```

This writes `.git/hooks/pre-push` in the current checkout only. It is not required for CI and it is not included in published source archives unless a user installs it locally.

## Good bug-fix discipline

Every meaningful bug fix should leave one of these behind:

- a unit/integration test;
- a Node harness assertion;
- a runtime trace;
- or a documented reason in the PR template explaining why the bug cannot be reproduced automatically yet.

For the bug lifecycle, read:

- `docs/bug-lifecycle.md`;
- `docs/bug-hunting-and-fix-playbook.md`;
- `docs/defect-taxonomy-and-labels.md`;
- `docs/maintainer-debugging-guide.md`.

## Recommended local tools

The project can still run its core scripts with only Node/npm and Rust, but a serious public release should also install:

```bash
cargo install cargo-nextest --locked
cargo install cargo-audit --locked
cargo install cargo-deny --locked
cargo install cargo-auditable --locked
```

Recommended use:

```bash
cargo nextest run --locked
cargo audit
cargo deny check
cargo auditable build --release --locked
```

These are not a replacement for `npm run verify:local-prepublish`; they are extra evidence for supply-chain and release quality.

## GitHub without a paid plan

The project does not require paid GitHub features to be checked locally. If the repository is public, standard hosted GitHub Actions runners, CodeQL code scanning, Dependency Review, private vulnerability reporting, and OpenSSF Scorecard can still be useful. They should be treated as extra public trust signals, not as the only proof path.

When GitHub is unavailable, run local gates and attach the generated reports to releases manually.
