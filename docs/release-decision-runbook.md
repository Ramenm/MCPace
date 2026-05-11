# Release decision runbook

This runbook separates three decisions that are often confused:

1. **Can the source tree be published or pushed publicly?**
2. **Can a native npm package be published?**
3. **Can README/product docs claim runtime beta or broader broker capability?**

MCPace should answer those questions with generated evidence, not vibes.

## Source snapshot decision

Run:

```bash
npm run prove:local-first
npm run verify:publish-decision
```

Allowed when:

- `reports/local-quality-source-latest.json` is `pass` or `pass-with-warnings`;
- `reports/secret-scan-latest.json` has zero critical findings;
- `reports/supply-chain-risk-latest.json` has zero blockers;
- `reports/free-tier-readiness-latest.json` has zero blockers;
- `reports/product-practice-latest.json` keeps source/runtime claims honest;
- `reports/publish-decision-latest.json.okForPublicSourceSnapshot` is `true`.

Warnings are allowed for a development source snapshot if they are documented. They should be resolved before a polished public launch.

## Native npm/runtime decision

Run on a host with Cargo dependency access and a supported target:

```bash
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
cargo build --release --locked
npm run stage:vendored-binary
npm run verify:vendored-binary
npm run verify:runtime-trace
npm run verify:local:full
npm run verify:publish-decision
```

Allowed only when:

- Rust quality is fresh and `pass`;
- a host-compatible native binary is staged and verified;
- runtime trace is fresh and `pass`;
- product-practice allows runtime/native install claims;
- `reports/publish-decision-latest.json.okForNpmNativePublication` is `true`.

## Claim decision

README, website, release notes, and npm descriptions must not say more than the reports prove.

Allowed current wording before native runtime proof:

- local-first MCP control plane;
- connectable runtime preview;
- source/control-plane surface strong;
- stdio upstream smoke paths implemented;
- HTTP upstream fan-out still blocked;
- native published install proof still blocked.

Blocked wording until proof exists:

- published binary install ready;
- runtime beta ready;
- universal remote MCP broker;
- public/relay-ready MCP gateway;
- HTTP upstream forwarding fully implemented.

## Final human review

Before publishing anything public, read these reports together:

```text
reports/local-quality-source-latest.md
reports/secret-scan-latest.md
reports/supply-chain-risk-latest.md
reports/product-practice-latest.md
reports/publish-decision-latest.md
```

For native releases also read:

```text
reports/rust-quality-latest.json
reports/runtime-trace-latest.md
reports/local-prepublish-latest.md
```

If a report is stale, missing, or generated on a different version, regenerate it.
