# Eval plan

## Eval goals

The eval suite should catch the failures that matter for this repo:

- roadmap work described as shipped product surface
- source-proof checks misreported as runtime or release proof
- fake precision in project status, percentage complete, or ETA
- scope jumps that outrun local runtime correctness
- compatibility claims made from brand names instead of documented surfaces
- pretty benchmark scores that hide unsupported claims
- autonomous-agent prompts that encourage unsafe “do everything” execution or unsupported release claims

The suite should reward a truthful answer that says **“not proven yet”** over a polished answer that guesses.

## Scenario matrix

Use `eval/scenario-matrix.json` as the machine-readable source of truth.

Current scenario families are:

- autonomous agent workloop and honest stopping conditions
- project-state reporting
- architecture and migration decisions
- routing and server arbitration
- client-surface compatibility
- proof and readiness claims
- packaging and release hygiene
- dependency/tooling governance
- runtime boundaries and contributor-vs-runtime scope

These are grounded in the repo’s current command surface, docs, reports, and historical corrections rather than in generic public benchmarks.

## Scoring rubric

Use `eval/scoring-rubric.json`.

Core rubric dimensions:

- task success
- factual support
- honesty and uncertainty handling
- scope control
- actionability

A material unsupported claim should fail the case even when the prose sounds good.

## Dataset plan

Use `eval/dataset-plan.json`.

Important rules:

- keep separate **typical / edge / adversarial / held-out** splits
- keep runtime-lab and seed-prompt tracks distinct
- prefer historical regressions and repo-grounded maintainer tasks over synthetic prompts
- treat raw production logs as **not yet available in-repo**; until then, use sanitized historical cases and production-like fixtures
- add sanitized real traces later instead of pretending the current dataset is already perfect

## Regression plan

For every prompt / tool / model / routing change:

1. run repo contract tests and npm tests
2. re-score the seed prompt/agent fixtures
3. inspect score deltas by family and by failure mode, not only by one summary metric
4. keep held-out cases out of routine tuning
5. calibrate rubric changes with humans before comparing against older runs

Before release-candidate claims:

- run held-out seed and runtime cases separately
- update verification artifacts with the exact host/toolchain proof that was actually executed
- do not let local source/archive success stand in for runtime or publish proof

## Main failure modes we will now catch

- confident guessing about runtime readiness, cross-platform support, or publish provenance
- exact ETA / percentage reporting without a real basis
- install/export or full hub parity described as already done
- synthetic eval suites that optimize for a vanity number instead of real maintainer work
- surface-specific client constraints hidden behind a single product name
