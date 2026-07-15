# Dashboard frontend architecture

The dashboard frontend is intentionally local, framework-free, and auditable. It has no bundler and no hidden build step. Rust embeds the following 11 source assets:

1. `src/dashboard/index.html` — the hidden/inert controller staging DOM and stable control IDs.
2. `src/dashboard/frontend/styles.css` — controller-level layout and compatibility styles.
3. `src/dashboard/frontend/product.css` — the canonical product shell, themes, responsive layout, focus, and accessibility states.
4. `src/dashboard/frontend/app.js` — shared state, API helpers, sanitization, preferences, and action primitives.
5. `src/dashboard/frontend/app.runtime.js` — runtime and resource helpers split from the core controller.
6. `src/dashboard/frontend/app.model.js` — server risk, policy, evidence, and operator-plan view models.
7. `src/dashboard/frontend/app.render.js` — primary controller rendering.
8. `src/dashboard/frontend/app.render.details.js` — detailed diagnostics and secondary rendering.
9. `src/dashboard/frontend/app.actions.js` — source, server, and client action handlers.
10. `src/dashboard/frontend/app.boot.js` — event wiring and controller bootstrap.
11. `src/dashboard/frontend/product.js` — the one visible product shell and its five-section interaction layer.

The two stylesheets load in the order shown. The eight scripts load with `defer` in this order: `app.js`, `app.runtime.js`, `app.model.js`, `app.render.js`, `app.render.details.js`, `app.actions.js`, `app.boot.js`, and `product.js`. This keeps the global plain-JavaScript model explicit while every `app*.js` chunk remains below the modernization budget.

## Canonical information architecture

There is one visible product shell and one `<main>`. It has exactly five destinations:

1. **Home** — current status, the next safe action, and the five-step foundation.
2. **Integrations** — MCP servers, routes, evidence, discovery/import/add flows, and the Server Atlas.
3. **Applications** — supported client applications and wiring state.
4. **Activity** — retained operations, outcomes, timing, and exports.
5. **Settings** — preferences and advanced operational settings.

`index.html` is not a second dashboard. Its controller root remains `hidden`, `inert`, and `aria-hidden="true"`; `product.js` moves only required live controls into the canonical shell. Do not add another visible shell, main landmark, navigation model, or legacy workspace switcher.

A server opens in one modal inspector with named tasks such as **Summary**, **Isolation**, **Setup**, and **Activity**. Raw launch/protocol details stay in the appropriate secondary task instead of competing with the routine path.

## Ownership rules

`/api/overview` owns product truth. The browser renders backend-owned readiness, access review, source rows, automation state, and diagnostics; it must not invent a second readiness model.

The browser may:

- validate simple fields before submission;
- navigate to a named destination and focus its heading;
- render loading and unavailable states;
- remember local display preferences;
- dispatch explicit JSON actions through dashboard API routes.

The browser must not:

- infer or reveal secret values from raw environment variables or headers;
- decide that a server is safe without backend evidence;
- build shell command strings;
- treat a known client catalog entry as a wired application;
- present derived plans as proven first-screen truth.

## Asset route contract

`src/dashboard.rs` serves:

- `GET /` → `text/html; charset=utf-8`
- `GET /dashboard.css` → `text/css; charset=utf-8`
- `GET /dashboard.product.css` → `text/css; charset=utf-8`
- `GET /dashboard.js` → `application/javascript; charset=utf-8`
- `GET /dashboard.runtime.js` → `application/javascript; charset=utf-8`
- `GET /dashboard.model.js` → `application/javascript; charset=utf-8`
- `GET /dashboard.render.js` → `application/javascript; charset=utf-8`
- `GET /dashboard.render.details.js` → `application/javascript; charset=utf-8`
- `GET /dashboard.actions.js` → `application/javascript; charset=utf-8`
- `GET /dashboard.boot.js` → `application/javascript; charset=utf-8`
- `GET /dashboard.product.js` → `application/javascript; charset=utf-8`

The Content Security Policy permits only same-origin dashboard scripts and styles. Do not reintroduce inline JavaScript. Inline style attributes remain temporarily allowed for small dynamic states, so `style-src` still includes `unsafe-inline`.

## Form, action, and safety rules

Forms use `novalidate` so product copy controls error text. Place errors next to the relevant field and explain both the problem and the correction. A disabled button must not be the only indication that input is invalid.

Safe setup flows stay explicit and preview-first:

- Import or discovery: Preview → Save disabled → Review → Enable → Test.
- Client wiring: Preview patch → Apply → Restore.
- Manual server add: Save disabled → Review → Enable and test.

Dynamic HTML must go through the reviewed sanitizer/escaping path. URL-bearing attributes are limited to fragments or credential-free same-origin HTTP(S). CSV exports must neutralize spreadsheet formula prefixes; JSON remains the full-fidelity export.

## Accessibility and responsive rules

Keep keyboard focus visible, preserve reduced-motion and forced-colors behavior, avoid horizontal scrolling, and keep primary touch targets at least 44 CSS pixels. Navigation uses real buttons with canonical accessible names. View changes update the hash, label the main landmark, and focus the active heading when appropriate.

Tabs use the complete tab pattern where tabs are appropriate. Pressed button groups use `aria-pressed` and matching visual selectors rather than mixing tab and toggle semantics. Global shortcuts must return immediately while any modal dialog is open. Actionable Undo notifications remain available until used or dismissed.

At narrow widths, the same five destinations remain available through the mobile navigation. Server rows become a single column and modal inspectors occupy the viewport without changing information priority.

## Verification

Before release:

- run the dashboard Node contract suite and syntax-check all eight JavaScript chunks;
- run `npm run proof:browser-lifecycle`;
- load the real dashboard in a headless browser and assert one product shell, one main landmark, five view hosts, and a hidden/inert controller root;
- verify no console errors and no failed asset requests;
- run axe/Lighthouse and manual keyboard/screen-reader checks in dark, light, system-light, monochrome, reduced-motion, forced-colors, zoomed, and narrow layouts.
