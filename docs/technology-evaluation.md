# Technology and execution evaluation

This note compares the main implementation paths that are still plausible for MCPace.

## Variant A — Flag-day runtime rewrite

**Approach**

Stop incremental work and jump straight to a full live runtime rewrite with ingress, leases, install/export, and release plumbing in one large push.

**Key steps**

- design the full runtime end-state up front
- replace read-path surfaces with live behavior in one sequence
- defer most proof until the full runtime exists

**Pros**

- cleanest end-state if it lands perfectly
- less transitional surface in theory

**Cons**

- highest regression and schedule risk
- weakest reversibility
- encourages overclaim because proof arrives late
- does not fit the current evidence-heavy, staged repo contract

**When it wins**

Only when the existing code is actively blocking progress and the team can afford a long, high-risk stabilization phase.

## Variant B — Incremental Rust-first completion inside the current repo (**recommended**)

**Approach**

Keep the current Rust control-plane/read-path baseline, add the runtime core in narrow slices, and preserve proof at each layer.

**Key steps**

- land `stdio-shim`
- land local Streamable HTTP ingress
- add lease/cancel/restart correctness
- re-run build/runtime proof on supported hosts
- only then add install/export and broader compatibility lanes

**Pros**

- best value/risk/reversibility ratio
- fits current code layout and tests
- keeps docs, evals, and source proof aligned with what is actually implemented
- makes it easier to stop at a meaningful checkpoint if blocked

**Cons**

- slower to produce a flashy “all new” story
- requires discipline to avoid partial-surface overclaim
- transitional docs need regular upkeep

**When it wins**

This wins whenever the repo already contains usable pieces worth preserving and the hardest work is still runtime correctness, not repo scaffolding.

## Variant C — Secondary core or new repo split

**Approach**

Create a new repo or a second implementation core (for example a heavier npm/TypeScript core) while the current repo remains as a compatibility shell.

**Key steps**

- define a new package/repo boundary
- duplicate or proxy the current command surface
- keep parity across two moving implementations

**Pros**

- can simplify publishing stories for one surface
- may isolate experimentation from the current tree

**Cons**

- highest long-term maintenance drift
- duplicates proof burden
- does not remove the need for ingress/lease/runtime correctness
- makes roadmap state harder to explain honestly

**When it wins**

Only if the current repo structure becomes a proven blocker after the runtime core is already stable.

## Recommendation

Choose **Variant B**.

It is the only path that matches the current repo reality:

- current value already lives in Rust read-path/control-plane code
- the biggest remaining risk is live runtime correctness, not packaging or repo shape
- the project already benefits from explicit proof layers and reversible steps

## What needs to be true before starting the next implementation slice

- a Rust-capable host exists for build proof
- the next slice is small enough to verify in isolation
- docs and evals are updated in the same pass so target-vs-current state stays honest

## Revisit conditions

Re-open this choice if any of these become true:

- the current repo layout starts blocking maintainability more than it helps
- a second core demonstrably reduces total complexity instead of adding drift
- runtime proof is complete but release/distribution constraints still cannot be solved inside the current structure
