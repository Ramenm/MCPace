# MCPace overhead/stability hardening final pass

Generated: 2026-05-19T10:54:49.697397Z
Status: **pass-with-known-native-and-install-lock-blockers**

## What this pass locks down

- Measured CLI launcher/native overhead, hot JSON-RPC/session/lock/tool-index paths, 100-server/100k-tool stress, and deterministic fixture lifecycle overhead.
- Kept random MCP packages disabled: no random server bins started, no tools/call sent, install scripts disabled in survey flows.
- Reused one signal-policy module across mass survey, adaptive profiling, and overhead benchmarks to avoid classifier drift.

## Key measurements

- Launcher overhead median delta: 170.83 ms; direct native median: 26.49 ms; npm launcher median: 197.32 ms; CLI source 14128 bytes; runtime deps 0.
- Hot path profile: JSON-RPC p95 15.267 µs; session key p95 2.393 µs; lock admission p95 2.827 µs; package policy p95 9.038 µs.
- Decomposition: route index speedup 113.18x; projection cache-hit speedup 24756.26x; scheduler 0.004 ms/op.
- Deep audit: config merge p95 35.918 ms; scheduler 3.058 µs/op; heap delta 15.659 MiB.
- Stress: 100 servers, 100000 tools, 25000 operations; indexed 100000 tools; random starts 0; active locks at end 0; executeDefault profiles 0.
- Live package survey: 100 npm MCP-looking packages; 10 tarballs downloaded with sha512 evidence; review required 68.
- Race audit: 15000 operations; violations 0; max active 17; blocked conflicts 735688.

## Checks

- nodeSyntax: pass
- targetedOverheadRaceTests: pass
- npmCliTests: pass
- sourceAudit: None
- adaptiveParallelism: pass
- adaptiveWorkerPlan: pass
- multiClientRuntime: pass
- lifecycleBlastRadius: pass
- performanceSmoke: pass
- overheadAudit: pass
- overheadProfile: pass
- overheadDecomposition: pass
- overheadPressure: pass
- overheadDeep: pass
- overheadBenchmark: pass
- overheadStress: pass
- fixtureLifecycleOverhead: pass
- massSurveyLive100: pass
- massInstallLockChunkedSmoke: blocked
- raceConditions: pass
- secretScan: pass
- npmPack: pass
- vendoredBinary: pass
- publishDecision: blocked

## Known blockers

- massInstallLockChunkedSmoke: blocked
- publishDecision: blocked

## Safety invariants

- randomMcpServersStarted: False
- randomMcpToolsCalled: False
- packageInstallScriptsAllowedInSurvey: False
- surveyedPackagesAutoEnabled: False
- nativeRustBinaryRebuiltHere: False
