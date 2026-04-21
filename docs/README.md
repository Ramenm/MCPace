# Documentation Index

This packaged copy tracks repo version `0.3.0`. Generated release archives use a
root folder named `<project-name>-v<version>-<ddmmyy-hhmmss>`.

Start here:

- `../README.md` — product overview and quick command surface
- `host-setup.md` — local prerequisites
- `toolchain-policy.md` — supported Node/npm/Rust lanes
- `test-strategy.md` — what to run for source and host proof
- `eval-plan.md` — prompt/agent eval goals, rubric, dataset, and regression loop
- `verification-matrix.md` — practical verification checklist
- `mcp-spec-alignment.md` — checked MCP baseline and transport scope
- `client-metadata-routing.md` — client/session routing inputs
- `client-surface-matrix.md` — local/cloud/API connector client differences
- `server-segmentation-and-auto-discovery.md` — server policy and serialization model
- `technology-decision.md` — Rust core + npm launcher rationale
- `technology-evaluation.md` — compared implementation paths and why incremental Rust-first completion still wins
- `rust-rewrite-architecture.md` — current module layout
- `rewrite-cutover-plan.md` — next implementation phases
- `runtime-lab.md` — runtime fixture lab and gaps
- `recovery-runbook.md` — stale/corrupt runtime recovery

Project-control docs at the repo root:

- `../TODO.md` — prioritized backlog and ETA ranges
- `../STATE.md` — current verified status and progress view
- `../DECISIONS.md` — active decisions and review triggers

Included machine-readable reports:

- `../reports/summary.md` — concise packaged summary
- `../reports/verification-latest.json` — latest machine-generated source/release verification snapshot
- `../reports/rust-command-coverage.json` — implemented vs planned command surface
- `../reports/toolchain-support.json` — stack policy used by CI and tests
