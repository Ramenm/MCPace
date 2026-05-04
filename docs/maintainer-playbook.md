# Maintainer Playbook

## Daily / per-PR loop

1. Read the issue or PR intent first: bug, feature, runtime proof, docs, repair, or cleanup.
2. Keep claims proof-scoped. Do not let a docs change imply runtime support that the reports do not prove.
3. Run the smallest relevant checks first, then the full source checks before merge.
4. Prefer one behavior change plus one proof update per PR.

## Triage

- Bugs need reproduction commands, OS/architecture, MCPace version, client, upstream type, and safe logs.
- Feature requests need a user problem and acceptance criteria.
- Runtime-proof issues should include the full `initialize -> tools/list -> tools/call` path.
- Security-sensitive reports move to `SECURITY.md` immediately.

## Merge checklist

```bash
npm test
npm run verify:github-readiness
npm run verify:product-practice
npm run verify:runtime-trace
npm run verify:rust-quality
```

Run Rust and runtime checks on a host with the pinned toolchain and dependency access. In constrained sandboxes, record what was blocked and why.

## Release checklist

1. Refresh source, boot, install-readiness, runtime-trace, rust-quality, product-practice, and GitHub-readiness reports.
2. Build release binary with `cargo build --release --locked`.
3. Stage and verify vendored/platform binaries.
4. Generate checksums and dry-run npm tarballs.
5. Create a draft GitHub Release before npm publishing.
6. Publish only through the trusted publishing workflow after the GitHub repository metadata matches.

## Community loop

Answer repeated questions by improving docs. Keep “good first issue” genuinely small. Keep `needs proof` visible until a claim has a test, runtime trace, or release artifact.
