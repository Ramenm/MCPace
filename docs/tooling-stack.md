# Tooling stack

This file explains what MCPace expects from the local development machine and why.

## Required

| Tool | Why it matters |
|---|---|
| Node 22+ / npm 10+ | Runs project automation, npm launcher checks, package dry-runs, and local proof harnesses. |
| Rust toolchain from `rust-toolchain.toml` | Builds the native `mcpace` binary and runs tests. |
| rustfmt | Keeps Rust code format stable. |
| Clippy | Catches Rust mistakes before tests and runtime traces. |
| git | Generates patches, release diffs, and local audit history. |

Check the machine:

```bash
npm run verify:tooling
```

## Recommended for serious release work

| Tool | Why it matters |
|---|---|
| cargo-nextest | Faster Rust test loops and better flaky-test diagnostics. |
| cargo-audit | Checks `Cargo.lock` against RustSec advisories. |
| cargo-deny | Enforces dependency policy: advisories, licenses, duplicate versions, sources, bans. |
| cargo-auditable | Embeds dependency metadata in native release binaries. |

## No paid GitHub dependency

MCPace can be proved locally. Hosted GitHub checks are useful once the repo is public, but release claims should never depend on a paid plan or hidden service. The minimum publish decision comes from:

```bash
npm run verify:local-prepublish
```

## When a tool is missing

Do not silently skip required proof. Record the missing tool in `reports/tooling-readiness-latest.json`, install it, and rerun the gate. In constrained sandboxes, it is acceptable to produce a blocked or partial report as evidence, but not to claim runtime beta or published install readiness from that report.
