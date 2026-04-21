# DECISIONS

## D-001 — Rust remains the product core; npm stays a launcher surface

**Decision**

Keep the Rust binary as the only product core. npm remains a distribution / launcher lane, not a second runtime implementation.

**Rationale**

- matches the current repo contract and current code layout
- keeps one correctness core instead of a split-brain runtime
- preserves the offline/minimal-dependency direction already visible in `Cargo.toml`

**Alternatives considered**

- **TypeScript/npm core**: easier packaging, but would create a second implementation center and re-open runtime divergence.
- **Dual-core Rust + TypeScript**: highest flexibility, worst maintenance and proof burden.

**Consequences**

- npm work should stay thin and packaging-oriented
- runtime correctness work belongs in Rust
- docs must not describe npm as the canonical runtime path

**Revisit when**

Only if Rust becomes a proven blocker for required runtime features and the repo has hard evidence that a second core reduces total complexity instead of adding drift.

---

## D-002 — Prefer incremental greenfield inside the current repo over a flag-day rewrite or repo split

**Decision**

Keep building the new hub/runtime inside the current repo with reversible stages and explicit proof gates.

**Rationale**

- the repo already has live migration evidence: grouped Rust commands, thin module roots, and staged source-proof checks
- a flag-day rewrite would trade current usable proof for a larger unverified surface
- an early repo split would increase coordination and parity drift without solving the hard runtime problems

**Alternatives considered**

- **Flag-day full rewrite**: fastest on paper, highest regression risk, weakest proof story.
- **Immediate new repo/fork**: cleaner history, but creates cross-repo drift before the design is stable.

**Consequences**

- every step should leave the repo in a runnable, testable state
- docs and evals must keep target architecture separate from current proof
- migration phases and exit criteria matter more than sweeping promises

**Revisit when**

Re-evaluate only if the current repo structure clearly blocks maintainability or release engineering after the runtime core has stabilized.

---

## D-003 — Keep one small grouped command taxonomy; aliases are temporary bridges only

**Decision**

The target public CLI stays grouped and small: `init`, `hub`, `client`, `server`, `profile`, `projects`, `verify`, plus later `repair` and `release`.

**Rationale**

- reduces interface sprawl
- matches the single-hub product direction
- keeps docs, help output, and install/export guidance comprehensible

**Alternatives considered**

- **Preserve every historical entrypoint as primary**: lower short-term migration friction, higher long-term confusion.
- **Many parallel top-level commands**: easier to bolt on, worse discoverability and maintenance.

**Consequences**

- compatibility aliases may exist, but only as migration aids
- new work should prefer grouped surfaces over one-off top-level commands
- docs must not treat alias count as product maturity

**Revisit when**

Only if user research later proves that the grouped taxonomy materially blocks adoption.

---

## D-004 — Separate source proof, build proof, runtime proof, and release proof

**Decision**

Do not collapse all verification into one pass/fail claim.

**Rationale**

- this repo can pass source-proof checks without having runtime proof
- local pack/archive success does not imply published provenance proof
- docs alone do not prove cross-host runtime behavior

**Alternatives considered**

- **Single “all green” badge**: simpler message, but it rewards overclaim.
- **Docs-only readiness claims**: fastest to write, least trustworthy.

**Consequences**

- verification artifacts should say exactly which proof layer passed or is blocked
- ETA and readiness claims must name missing layers explicitly
- held-out evals are useful because they catch release-time overclaim pressure

**Revisit when**

Never remove the proof-layer split; only refine how each layer is checked.

---

## D-005 — Local-first runtime correctness comes before cloud relay, web UI, or desktop shell work

**Decision**

Finish the local runtime correctness slice before broadening the product surface to cloud relay, web UI, or desktop packaging.

**Rationale**

- current highest-risk gaps are still in ingress, leases, cancellation, and host proof
- extra surfaces would amplify unproven behavior rather than resolve it
- the client catalog already shows how much local/cloud behavior diverges

**Alternatives considered**

- **Cloud-first expansion**: attractive for reach, but premature before local correctness exists.
- **Desktop/web shell first**: visible demo value, weak engineering leverage right now.

**Consequences**

- local runtime proof is the gating path to release-candidate quality
- cloud/public HTTP relay remains explicitly deferred work
- roadmap and evals should punish scope jumps that outrun proof

**Revisit when**

After local ingress, lease management, and host/runtime proof are in place.

---

## D-006 — Eval design must reward grounded honesty more than confident guessing

**Decision**

Use production-like, repo-grounded evals with separate typical / edge / adversarial / held-out splits, binary gates, rubric scoring, and human calibration.

**Rationale**

- the main failure mode in this repo is not eloquence; it is **unsupported certainty**
- a single vanity metric hides the difference between a careful answer and a persuasive hallucination
- held-out sets matter because the repo evolves quickly and easy daily cases are easy to overfit

**Alternatives considered**

- **Generic public benchmark only**: easier to report, weak connection to real maintainer work.
- **One summary metric**: easy to optimize, hard to trust.
- **Vibe-based manual review only**: flexible, not reproducible.

**Consequences**

- unsupported-claim rate and uncertainty quality are first-class metrics
- some cases should intentionally pass by saying “not proven yet” when evidence is missing
- eval fixtures need grounding metadata and evidence paths, not just prompts

**Revisit when**

Refine the rubric when enough real traces exist, but keep the anti-overclaim bias.

---

## D-007 — Preserve external session ids, but keep fallback lease ids internal and deterministic at planning time

**Decision**

When a client already supplies a session id, keep it visible as the dominant routing seed. When no explicit session id exists, the current planner may derive a deterministic internal lease id from stable context, but that fallback is a **planning/runtime partition key**, not a claim that the final public MCP HTTP session header should be deterministic.

**Rationale**

- `client plan` needs a sticky routing key before the live ingress exists
- preserving explicit external ids keeps planner output explainable
- deterministic planner output keeps tests and diffs stable while the runtime is still incomplete

**Alternatives considered**

- **Anonymous shared bucket when no session id exists**: simpler, but unsafe for sticky routing and impossible to reason about in planner output.
- **Random planner-generated lease every run**: more opaque, harder to test, and noisy for dry-run planning.
- **Treat planner fallback as the final public MCP session id**: overreaches beyond what the current runtime actually proves.

**Consequences**

- planner output stays reproducible
- external ids remain distinct from internal fallback ids
- future HTTP ingress must still make an explicit decision about protocol-facing session handling and security
- docs must not imply that `planned:<hash>` is equivalent to `Mcp-Session-Id`

**Revisit when**

Revisit as soon as live Streamable HTTP ingress lands or the repo adds protocol-facing session minting.

---

## D-008 — Readiness must follow required runtime lanes, not mere config presence

**Decision**

`runtime_prerequisites_ready` should only be true when the project config exists **and** the prerequisites for the actually required runtime lanes are available. In the current repo slice, runtime-enabled `container-*` servers make Docker a readiness dependency.

**Rationale**

- a config file alone is not runtime readiness
- the previous config-only signal would overclaim green readiness for container-backed server sets on hosts without Docker
- optional tools should not block readiness unless an active runtime lane depends on them

**Alternatives considered**

- **Config-only readiness**: easiest to compute, least trustworthy.
- **Require every known optional tool always**: safer-looking, but too pessimistic and noisy.
- **Delay readiness reporting until the full runtime exists**: avoids false green, but gives up useful early diagnostics.

**Consequences**

- readiness output is more trustworthy today
- future runtime kinds will need explicit prerequisite mapping instead of piggybacking on Docker logic forever
- docs/tests should keep the distinction between config presence and executable runtime readiness clear

**Revisit when**

Revisit when `stdio`/HTTP ingress lands and additional runtime kinds add their own prerequisite sets.

---

## D-009 — Verification snapshots must be generated from executed checks, not hand-edited

**Decision**

Use a machine-generated verification snapshot (`reports/verification-latest.json`) produced by `scripts/proof-report.mjs` for the current environment instead of manually editing proof status by hand.

**Rationale**

- proof-layer drift is easy when source/release status is copied into docs or reports manually
- the repo already separates source, build, runtime, and release proof, so the artifact should preserve that separation automatically
- the current container can execute source/release proof but cannot honestly claim Rust/runtime proof

**Alternatives considered**

- **Keep editing the JSON report manually**: fastest once, but it drifts and rewards wishful status updates.
- **Collapse everything into one green/red status**: simpler to read, but it hides blocked proof layers and encourages overclaim.
- **Do not keep a latest snapshot at all**: avoids drift, but throws away a useful machine-readable status artifact for packaging and review.

**Consequences**

- `npm run prove:report` becomes the canonical way to refresh the latest verification snapshot in environments that can at least run source/release proof
- blocked proof layers stay visible instead of being silently omitted
- future host/runtime automation can extend the same report rather than inventing a second status format

**Revisit when**

Revisit when real Rust-host and runtime automation are available and should be folded into the same report script instead of staying blocked/not-run.

