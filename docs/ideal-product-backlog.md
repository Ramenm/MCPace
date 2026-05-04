# Ideal Product Backlog

This backlog is ordered by impact on trust, adoption, and runtime correctness.

## P0: make the current promise undeniably true

- Fix every proof harness drift before adding new feature surface.
- Keep runtime trace target-aware and current-host aware.
- Make product-practice reject stale, wrong-target, or legacy runtime reports.
- Keep README, `ROADMAP.md`, and `docs/product-truth-and-beta-gate.md` aligned.
- Keep default upstream server config empty and BYO.

## P1: make the local runtime beta-worthy

- Fresh proof for the in-process `/mcp` session store.
- Compatibility traces for `Mcp-Session-Id` create/reuse/close behavior.
- HTTP upstream connector.
- Real-client runtime trace.
- Better dashboard empty states and next commands.
- Safer non-local bind behavior with mandatory auth or hard blocking.

## P2: make installation boring

- Platform package proof for Linux, macOS, and Windows.
- Checksums, attestations, and npm trusted-publishing readiness.
- One-minute quickstart after published packages exist.
- Demo GIF or terminal recording.
- Troubleshooting guide from `mcpace connect` and `mcpace verify doctor` outputs.

## P3: make contribution easy

- Label starter issues as `good first issue` only when they have bounded acceptance criteria.
- Keep bug, feature, docs, runtime-proof, repair, and cleanup templates active.
- Convert repeated support questions into docs.
- Maintain a small public changelog or release notes path.

## P4: future expansion

- Remote/public relay mode with explicit auth and threat model.
- Team policy and audit logs.
- Advanced routing by client, project, session, and upstream risk class.
- Optional package-manager integrations beyond npm once npm is proven.
