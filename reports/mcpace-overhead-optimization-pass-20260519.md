# MCPace overhead optimization pass — 2026-05-19

Status: **source-pass-release-blocked**

## Changes
- Optimized large tool-scale simulation to use lazy compact tool materialization and shared bounded top-k instead of retaining/sorting an unbounded catalog.
- Added package metadata classifier fingerprint cache in the overhead decomposition path to avoid repeated signal inference during registry/UI refreshes.
- Added warmup to overhead pressure measurement so cold JIT does not create false profile-throughput blockers.
- Fixed deep overhead report output handling for absolute temp paths and removed literal tool-call wording from the no-execution proof source test.
- Simplified docs/README overhead links to reduce duplicated references; kept canonical overhead doc plus focused pressure note.
- Kept quick overhead gates separate from full/deep overhead gates so local orchestration checks do not repeatedly pay every heavyweight benchmark.

## Measurements

| Area | Status | Result |
|---|---:|---|
| Performance smoke | pass | HTTP p95 max 51.73 ms; tool-scale 210 ms |
| Tool scale | pass | 50 servers / 200000 tools; elapsed 316 ms; full catalog materialized=false |
| Overhead decomposition | pass | route speedup 113.18x; classifier 0.0096 ms/op |
| Pressure | pass | profile 10.061 us; scheduler 1.238 us |
| Deep 100-server audit | pass | cached profile 0.356 us/server; route lookup 0.914 us |
| Stress | pass | 100 servers / 100000 tools / 25000 ops |
| Race audit | pass | 15000 ops; violations 0 |
| 100-package survey | pass | 100 packages; review-required 68; tarballs 10 |
| 100-package install-lock smoke | blocked | attempted 20/100; ok=false |
| Current live memory probe | pass | 9 tools; policy mismatches 0 |

## Verification
- nodeSyntax: pass
- targetedOverheadAndRaceTests: pass, 30/30
- npmTest: pass for audit/source smoke/npm CLI; bug-sweep warning only for missing Rust proof
- fullNodeRepoAttempt: not counted as full pass in this sandbox: long full test run was interrupted; sharded shard 1/4 passed 17/17 before sandbox interruption
- vendoredBinary: pass
- npmPack: pass
- secretScan: pass
- supplyChain: pass-with-warnings
- publishDecision: blocked

## Blockers
- Rust toolchain/cargo is unavailable in this sandbox, so native Rust rebuild and rust-quality proof remain blocked.
- Public native release remains blocked by publish-decision until Rust build/test/runtime-trace proof is refreshed on a release host.
- 100 arbitrary-package install-lock resolution remains blocked/too heavy under the safe chunked budget; package scripts stayed disabled and no MCP servers were started.
- Re-running multiple live MCP package probes in this sandbox hit package-manager/runtime timeouts; one current official memory probe passed, while the safe 100-package survey remains metadata/tarball-only.
