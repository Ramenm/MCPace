# Dashboard base model

The dashboard should stay boring on purpose. The first screen is not a command center, a proof wall, or a complete diagnostics console. It should answer one setup question: what is the smallest safe path from a local client to usable tools?

## First screen order

Show the foundation in this order:

1. **Backend** — the local dashboard/API process is reachable. This is not the same as runtime readiness.
2. **Client** — at least one local client is wired to MCPace, not merely known in a support catalog.
3. **Source** — at least one MCP server source exists in configuration.
4. **Tools** — enabled sources have recent `tools/list` evidence or a clear post-enable test action.
5. **Routing** — policy is safe enough to use the source.

Everything else is secondary until these five basics are understandable.

## Data ownership

`/api/overview` owns the base model through `dashboardFoundation`. The frontend may derive a fallback while loading, but it should not independently invent a different first-screen model.

The model is intentionally small:

- `steps` describes the five visible basics.
- `nextStep` is the single backend-selected step the UI should name first.
- `actions` describes the next safe actions.
- `displayRules` documents what belongs on the first screen and what stays folded.
- `counts.backendReachable` and `counts.runtimeReady` stay separate so the UI does not call a runtime/setup problem a backend outage.

The contract is documented in `schemas/mcpace-dashboard-foundation.schema.json`; update it when `/api/overview.dashboardFoundation` changes.

## What stays folded

Keep these in expandable tools or diagnostics, not the first screen:

- automation internals;
- discovery tuning;
- protocol compatibility details;
- runtime control and resource monitoring;
- raw evidence, raw logs, and proof artifacts;
- advanced routing and worker overrides.

These are still important, but they should not compete with the base setup path.

## Action rules

Prefer one visible next action over a long review queue. A good dashboard action should be explicit, reversible when possible, and close to the field it uses.

Safe default setup order:

1. Import an existing local client config.
2. Save imported sources disabled.
3. Review source, remote/auth hints, and secret names.
4. Enable deliberately when the user is ready to probe.
5. Run Test immediately; the row may offer **Enable & test** as one confirmed action.
6. Apply routing policy.

Client wiring should also follow preview-first behavior: preview patch, apply explicitly, restore from backup if needed.

Disabled sources are parked. Do not promise that a parked source has tools evidence; Test is meaningful after the source is enabled/routable, and the dashboard should make that order explicit.


## Architecture boundary

The dashboard frontend should render and validate forms, but it should not own product truth. The backend should own derived readiness, base-step status, and action availability. CLI actions should remain behind dashboard API routes rather than leaking shell strings into the browser.

Do not conflate layers:

- `/api/overview` responding means the dashboard backend is online.
- runtime prerequisites belong to the **Routing** or **Use** step.
- client catalog support does not prove that a client config is wired.
- a saved source does not prove usable tools until `initialize` and `tools/list` evidence exist.

## Copy rules

Use plain product words on the first screen:

- Backend
- Client
- Source
- Tools
- Routing
- Status
- Test
- Enable
- Details

Avoid vague or overconfident words on the main path, especially when the system cannot prove them numerically: confidence, proof, cockpit, operator, intelligence, autonomous, magic.

## v12 foundation hardening

The base checklist should not turn green merely because no data exists. Dependent steps must wait for their prerequisite evidence:

- Backend can be green when `/api/overview` responds; runtime readiness is reported separately.
- Routing cannot be green until runtime is ready, at least one source exists, tools evidence exists, and there are no policy blockers.
- Primary navigation must not point to hidden legacy sections. Hidden panels may remain for compatibility, but they should be reached only through explicit reveal actions.
- Dashboard action lists should be de-duplicated before rendering so the first screen does not show the same task twice.

This keeps the base path honest: online API, wired client, saved source, verified tools, then safe routing.

## v13 action-label discipline

The first-screen step buttons must not all say **Open**. Each foundation step now carries backend-owned `actionLabel`, plus `stateKey` and `nextStepKey`, so the frontend renders a specific action without reinterpreting product state:

- Backend: Refresh or Check link.
- Client: Connect or Open client.
- Source: Import or Open sources.
- Tools: Run test or Open tools.
- Routing: Repair, Review, or Open routing.

This is intentionally small. It does not add another panel; it makes the existing base checklist more concrete and keeps the browser from inventing labels that can drift away from the backend model.

## Foundation hardening rules

The base screen should not grow new visible concepts unless they reduce a first-run failure. Prefer improving one of the five existing steps over adding another panel.

`dashboardFoundation.safety` is backend-owned. The browser may render counts and names, but it must not infer or reveal secret values from raw env, headers, tokens, or authorization data.

Dashboard action routes validate boundary inputs before building CLI argv:

- server names cannot be empty, control-bearing, too long, or start with `-`;
- discovery mode is an explicit enum, not a free string;
- routing `reusePolicy` is an explicit enum;
- affinity entries are short tokens with a small count limit.

Automation should be reliable but not magical. Auto jobs can run in parallel for speed, but a failed syntax child is retried serially before reporting failure, because constrained runners can produce transient spawn/cwd failures that are not code failures.

## Access review boundary

The access review is allowed immediately after the five basics and the signal strip. It is not a sixth base step and it should not become a new command center. Its job is only to answer: before a source is enabled or routed, what access boundary should the user review?

The smaller `dashboardFoundation.safety` block is the pre-enable reminder inside the base panel. Keep it short: evidence count, remote/http count, secret-name count. It exists to stop a dangerous green state before the user reaches server rows; it must not become another permissions editor.

`/api/overview.accessReview` is backend-owned for the same reason as `dashboardFoundation`: the browser should not independently decide whether tools, credentials, remote origins, or missing evidence are safe. The frontend may render a fallback while loading, but the durable model comes from the backend. The public shape is documented in `schemas/mcpace-dashboard-access-review.schema.json`.

Keep the visible summary small:

- **Approval** — destructive, mutating, open-world, credential, network, filesystem, unknown, or sampling-like paths need explicit user review.
- **Secrets** — show env/header names and counts only; never show values.
- **Remote/Auth** — remote HTTP and credential-backed sources need origin/scope review.
- **Evidence** — enabled sources should not look normal until initialize/tools-list evidence exists.

This is a boundary check, not a permissions editor. Detailed auth scopes, raw headers, tool annotations, process data, and protocol diagnostics stay folded.


## v14 base-order hardening

Do not let runtime repair jump ahead of the five-step base order. Runtime readiness matters, but it belongs to the **Routing** step. If the backend is online and the client is not wired, the primary action is **Connect client**, even when runtime prerequisites are also not ready. If there is no saved source, the primary action is **Import**. If no tools evidence exists, the primary action is **Run test/Open servers**. Only when Backend, Client, Source, and Tools are understandable should the base panel ask the user to repair runtime or review routing.

Routing is not ready merely because a source exists. At least one source must be enabled, tools evidence must exist, runtime must be ready, and there must be no blockers or policy fixes waiting. Parked sources are useful configuration, not normal routing.

## v15 frontend baseline

The dashboard shell, CSS, and JavaScript are now separate source assets under `src/dashboard/`. This is not a new build pipeline; Rust still embeds the assets and serves them locally. The split exists to protect the base architecture: semantic DOM stays visible in `index.html`, visual rules stay in `frontend/styles.css`, and browser behavior stays in `frontend/app.js`.

Bulk policy and derived backend plans stay inside the folded **Advanced policy and plan** drawer. Routine use should go from the base setup path to server rows, then open setup or diagnostics only when needed.
