# Eval regression readiness pass — 2026-05-16

## What already exists

- `docs/eval-plan.md` defines goals, scenario families, rubric dimensions, dataset plan, and regression loop.
- `eval/scenario-matrix.json` maps project-state, architecture, routing, client compatibility, proof/readiness, packaging, dependency/tooling, and runtime-boundary families.
- `eval/scoring-rubric.json` defines good/bad/unacceptable behavior and metrics including task success, unsupported claims, uncertainty handling, optional latency, and optional cost.
- `eval/dataset-plan.json` separates seed prompt and runtime-lab tracks, with typical, edge, adversarial, and held-out splits.
- `eval/fixtures/seed/*.json` and `eval/fixtures/runtime/*.json` provide repo-grounded cases.
- `tests/node/eval-contract.test.js` and `tests/node/fixtures-contract.test.js` enforce structural integrity.

## Real-work coverage

Covered:

- project state and release-readiness overclaim;
- architecture migration traps;
- client surface and cloud/local compatibility boundaries;
- unsafe server sharing and runtime ownership conflicts;
- packaging state leaks and trusted-publishing overclaim;
- environment isolation and diagnostic leakage regressions.

Not yet fully covered:

- automated model execution over the seed fixtures;
- cost/latency measurement for real model/tool runs;
- sanitized production logs from actual maintainer traffic;
- human calibration records for held-out cases.

## Recommended regression loop

For every prompt, tool, routing, or model change:

1. Run source contract checks: `npm run test:repo:smoke` and `npm run test:npm`.
2. Run full Node repo tests when the change touches contracts: `npm run test:repo`.
3. Re-score seed fixtures against binary requirements and the rubric.
4. Compare unsupported-claim rate before comparing prose quality.
5. Keep held-out cases for release-candidate or major behavior changes.
6. Require human review when a case moves from blocked to supported, or when a release claim changes.

## Suggested next implementation

Add `scripts/eval-fixture-check.mjs` as an offline runner that:

- validates fixture schema;
- checks required evidence paths exist;
- reports split coverage and unsupported-claim guardrails;
- writes `reports/eval-fixture-check-latest.json`.

Only after that should the repo add a provider-backed model eval runner, because model calls introduce credentials, cost, latency, and nondeterminism.
