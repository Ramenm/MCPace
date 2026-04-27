# Product truth and beta gate

This document fixes the current-cycle product contract so README, `STATE.md`,
roadmap, and the runtime capability inventory do not drift apart.

The same contract is also mirrored machine-readably in `docs/product-truth.json`
so proof reports and doc contracts can detect drift automatically. That JSON now
keeps a selector for proof-tier-selected surfaces instead of hardcoding a fixed client
list.

## Current-cycle release promise

The strongest honest public promise today is:

**One local MCPace endpoint, simpler install on selected local clients, and
honest diagnostics for what is configured versus actually usable.**

That is narrower than the north star in
`docs/universal-mcp-runtime-north-star.md`. Until live ingress, lease
ownership, stale-result guards, and real-host proof exist, do not collapse the
north star into a present-tense claim.

## First ICP

Treat the first ICP as:

- **advanced integrator / solo power user first**;
- working across **2–3 local MCP clients**;
- feeling pain from hand-maintained config drift;
- willing to run one local MCPace process if it gives one URL, clearer routing,
  and actionable diagnostics.

Team operator, cloud relay, and enterprise policy stories remain follow-on
lanes, not the first wedge.

## Activation

Use two separate activation concepts so the product does not overclaim.

### Activation that is measurable today

A user is activated today when all three are true:

1. `mcpace client install <surface>` or `mcpace client export <surface>`
   succeeds;
2. `mcpace serve --port 39022` exposes `http://127.0.0.1:39022/mcp`;
3. the chosen client reaches MCPace and completes at least
   `initialize -> tools/list` against that localhost endpoint.

### Activation required for beta truth

Beta-quality activation is stricter:

1. a real client reaches MCPace;
2. MCPace resolves session/project ownership correctly;
3. at least one upstream tool call succeeds without stale-result or ownership
   confusion.

Until that second definition is proven on supported hosts, do not talk as if the
current connectable endpoint equals a fully proven runtime.

## Product shape for the current cycle

Treat the current product as:

- **local-first control plane plus onboarding layer**;
- with a **connectable local MCP endpoint**;
- and **preview runtime lanes** where proof is still incomplete.

Do **not** treat the current cycle as:

- universal runtime already proven;
- hybrid local + remote platform already shipping;
- team-wide control plane with enterprise guarantees.

## Entry-point contract

The product mental model for the next cycle is:

- **`serve` is the product**;
- **`hub` is internal/operator-facing lifecycle machinery**;
- **`dashboard` is an optional view into state**.

Keep the user-facing story close to:

**one binary, one port, one localhost MCP URL, one place to inspect health.**

## Surface priority

For the next cycle, support priority is catalog-driven rather than hardcoded in
this document.

- any surface with `proofTier = tier-1` in `src/client_catalog.rs` is
  part of the current proof gate and must receive real-host traces, drift
  checks, and docs priority before beta;
- local surfaces marked `proofTier = tier-2` may keep working
  config-writing install paths, but they should not outrank runtime correctness
  or current-cycle proof work;
- `proofTier = catalog-only` or `proofTier = generic` surfaces remain
  preview/manual until relay/auth/runtime proof exists.

This keeps the proof gate extensible: new client surfaces can be promoted by
updating the client catalog metadata instead of rewriting every truth document,
and install-capable local surfaces can be widened from the same metadata rather
than hand-editing parallel client lists.

## Public truth taxonomy

The capability inventory now keeps two distinct fields:

- `status` — implementation completion of the full capability definition
  (`implemented`, `planned`, `missing`)
- `claimStatus` — the strongest honest public claim right now

Use these `claimStatus` values consistently:

- `supported` — safe to describe as working now inside the current repo proof
  boundary
- `supported-local-only` — working for documented local lanes, but not a broad
  cross-surface guarantee
- `control-plane-only` — read/lifecycle/control behavior exists, but not the
  live runtime semantics implied by the bigger capability name
- `bootstrap-only` — bootstrap or attach proof exists, but not live forwarding
- `connectable-preview` — users can reach a visible MCPace endpoint/contract,
  but runtime/session/host proof is still incomplete
- `requires-host-proof` — source-level behavior exists, but the support claim
  waits on real supported hosts
- `planned` — not yet strong enough to expose as working behavior

Important: today `mcpace lab` still treats only `status = implemented` as fully
covered. `claimStatus` exists so docs, roadmap, and product messaging can stay
honest while the richer source-side truth model catches up.

## Beta gate

Do not call the product beta until all of the following are true:

1. catalog-selected `proofTier = tier-1` surfaces have real-host traces;
2. local HTTP ingress proves session creation/reuse/close behavior;
3. lease ownership exists for `single-session` / `shared-exclusive` lanes;
4. cancel/restart/stale-result guards exist;
5. `client install/export` has dry-run or diff semantics and rollback guidance;
6. capability inventory, README, `STATE.md`, and docs use the same public truth
   taxonomy.

## GA gate

Do not call the product GA until beta conditions hold and:

1. at least two OS lanes have repeated runtime proof;
2. deprecation/support policy exists for client surfaces;
3. a safe diagnostic bundle/redaction contract exists;
4. release proof includes published artifact/provenance checks instead of only
   source packaging proof.

## Explicitly out of scope for the next two quarters

Keep these out of the current-cycle promise unless proof lands first:

- public cloud relay as a supported product lane;
- broad “universal runtime” positioning;
- enterprise/team policy as the main story;
- long-tail client-surface breadth beyond the catalog-driven proof plan above;
- remote/local SKU splitting before the local runtime slice is proven.
