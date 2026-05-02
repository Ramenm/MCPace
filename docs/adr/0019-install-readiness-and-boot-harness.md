# ADR 0019 — Install readiness and boot harness

## Context

MCPace now has useful BYO MCP onboarding commands and data-driven presets, but install readiness was scattered across source audit, npm pack verification, Rust quality checks, manual inventory reports, and release scripts. That made it easy to overclaim readiness when only part of the source/proof lane had passed.

## Problem / goal

Provide one deterministic, low-risk harness that answers:

- Is the source tree internally consistent?
- Are first-use assets present?
- Does npm pack produce the expected CLI package shape?
- Is the current toolchain acceptable for the project policy?
- Is the package a thin launcher or a staged native-binary distribution?
- What is the exact next action before install/runtime claims?

## Constraints and non-goals

- Do not publish or install anything.
- Do not require Cargo dependency downloads to run the source inventory or npm pack probe.
- Do not turn a thin launcher into a claimed native binary package.
- Keep runtime proof separate from install/source readiness.

## Considered options

1. Keep manual reports only. This is low effort but too easy to drift.
2. Put all checks inside `proof-report.mjs`. This centralizes output but makes a heavy command the only useful probe.
3. Add a small boot harness and a public install-readiness wrapper. This gives a fast source/install readiness check without replacing full proof/report scripts.

## Decision

Use option 3.

New commands:

```bash
npm run inventory:source
npm run inventory:project
npm run verify:boot
npm run verify:install-readiness
```

New schema markers:

```text
mcpace.sourceInventory.v1
mcpace.codeInventory.v2
mcpace.bootHarness.v1
mcpace.installReadiness.v1
```

## Consequences / risks

- The boot harness intentionally reports `partial` when Node/npm are below policy or no native binary is staged.
- The npm package can be verified as a thin launcher even when published install readiness is not complete.
- Runtime readiness still requires Cargo check/test/build and a real MCP client trace.

## Plan

1. Keep boot harness in the standard source proof script set.
2. Use `install-readiness-latest.json` as the quick install status artifact.
3. Stage native binaries/platform packages before claiming published npm install readiness.
4. Record runtime trace before beta claims.

## Open questions

- Whether the default public install path should be source-build, native binary packages, or both.
- Whether CI should fail on boot harness warnings or only on blockers.
