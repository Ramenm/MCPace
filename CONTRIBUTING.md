# Contributing

Thanks for helping improve MCPace. The project is proof-driven: a change is not done just because it compiles; it should also have the right test, diagnostic, and documentation surface for the claim it makes.

## Working assumptions

- The active implementation core is Rust.
- npm exists only as a thin launcher/install surface.
- PowerShell is no longer part of the active repository contract.
- MCPace uses a Bring Your Own MCP servers model. Do not add arbitrary upstream servers as enabled defaults.
- `serve` is the product surface. `hub` is internal/operator-facing lifecycle machinery.

## Supported contributor stack

- Node 22 LTS or Node 24 LTS
- npm 10+
- Rust 1.95.0 from `rust-toolchain.toml`

The preferred local default is Node 24 from `.nvmrc` / `.node-version`. See `docs/toolchain-policy.md` and `reports/toolchain-support.json` for the policy and upgrade rules.

## Minimum contributor workflow

Run source-level checks first:

```bash
npm test
npm run pack:npm:dry-run
```

On a host with the Rust toolchain and dependencies available:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
cargo build --release --locked
```

For runtime/release work, also run the proof gates that match the claim:

```bash
npm run verify:boot
npm run verify:install-readiness
npm run verify:runtime-trace
npm run verify:product-practice
```

## What good PRs include

A good PR keeps facts, assumptions, and proof layers separate:

- a short reason for the change;
- the user-visible behavior that changes;
- tests or proof reports;
- docs updates when the public contract changes;
- a clear `Not tested` section if any important proof lane could not run.

## Review rules

- Keep facts, assumptions, and proof layers separate.
- Do not reintroduce deleted PowerShell entrypoints.
- Keep docs, tests, manifests, local version files, CI, and reports aligned with the actual repo state.
- Do not expand README/release claims without matching proof.
- Do not silently weaken environment isolation, redaction, origin checks, or local-first defaults.
- Do not make remote/public endpoint behavior look supported until the required security gates are implemented.

## Issue guidance

Use the issue templates when possible:

- bug reports need reproduction steps, expected/actual behavior, version, OS, client, and upstream type;
- feature requests should describe the user problem and the proof needed to call the feature done;
- security vulnerabilities should follow `SECURITY.md`, not public issues.

## Contributor-friendly areas

Good first contributions are usually:

- docs that clarify an existing proven behavior;
- small CLI diagnostic improvements;
- tests for already specified behavior;
- platform/package verification improvements;
- examples that do not expand product claims.

Runtime/session/security changes are welcome, but they need stricter review because MCPace can touch local files, credentials, and developer workflows through configured upstream servers.


## Contribution lanes

Use the smallest lane that proves the change:

- Documentation-only: update docs and run `npm run lint:npm` when scripts or examples are touched.
- Source hygiene: run `npm test` and include the changed proof report when applicable.
- Runtime behavior: add or update Rust/Node tests and run the narrow runtime suite first.
- Release/distribution: run the platform-package and release-target verification commands before opening a PR.

## Proof expectations

A PR should say what it proves and what it does not prove. Do not mark a feature as shipped only because docs were updated. For runtime claims, include the command that exercises the path, for example `npm run verify:runtime-trace`, `npm run verify:product-practice`, or a targeted Rust test.

## Safe config mutation

Any change that writes user or client configuration should preserve this contract:

- dry-run first;
- show a diff where possible;
- create a backup before writing;
- support restore or rollback;
- never overwrite user-managed blocks silently.
