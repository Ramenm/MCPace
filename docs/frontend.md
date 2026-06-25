# Dashboard frontend architecture

The dashboard frontend is intentionally small and boring. It has no bundler, no client framework, and no hidden build step. Rust embeds three source assets and serves them from the local dashboard HTTP surface:

- `src/dashboard/index.html` — semantic shell and stable DOM ids.
- `src/dashboard/frontend/styles.css` — visual system, layout, accessibility states, and responsive rules.
- `src/dashboard/frontend/app.js` — rendering, local preferences, form validation, and explicit action dispatch.

This keeps the project easy to audit while avoiding the previous single-file dashboard drift. The HTML shell should stay readable; CSS and JavaScript can change without burying structure inside one 8k-line file.

## Ownership rules

`/api/overview` owns product truth. The frontend renders `dashboardFoundation`, `accessReview`, server rows, automation status, and diagnostics, but it must not invent a different readiness model.

The browser may:

- validate simple fields before submission;
- focus the relevant field or drawer;
- render fallback loading states;
- remember local display preferences;
- dispatch explicit JSON actions through dashboard API routes.

The browser must not:

- infer secret values from raw env or headers;
- decide that a source is safe without backend state;
- build shell command strings;
- treat a known client catalog target as a wired client;
- show advanced derived plans before the base setup path.

## First screen contract

The first visible model remains:

1. Backend
2. Client
3. Source
4. Tools
5. Routing

Access review may appear after this foundation as a compact boundary check. Server rows come before setup drawers. Bulk policy plans, runtime internals, protocol diagnostics, and automation internals stay folded.

## Asset route contract

`src/dashboard.rs` serves:

- `GET /` → `text/html; charset=utf-8`
- `GET /dashboard.css` → `text/css; charset=utf-8`
- `GET /dashboard.js` → `application/javascript; charset=utf-8`

The Content Security Policy allows external same-origin dashboard scripts and styles. Inline JavaScript should not be reintroduced. Inline style attributes are still tolerated for small dynamic state, so `style-src` keeps `unsafe-inline` until those attributes are removed.

## Form and action rules

Forms use `novalidate` so dashboard copy controls the error text. Errors should be placed next to the relevant field and explain what went wrong and how to fix it. Buttons can show a busy label while a JSON action is in flight, but a disabled button must not be the only way the user learns what is wrong.

Safe setup flows stay explicit:

- Import: Preview → Save disabled → Review → Enable → Test.
- Discovery: Preview → Save disabled → Review → Enable → Test.
- Client wiring: Preview patch → Apply → Restore.
- Manual server add: Save disabled → Review → Enable & test.

## Accessibility and responsive rules

Keep keyboard focus visible, avoid horizontal scrolling, preserve reduced-motion behavior, and make primary touch targets large enough for mobile use. Dense system information should be grouped into rows, cards, or drawers only when the grouping makes the next action easier to find.

## Screenshot QA notes

The frontend QA pass renders the dashboard with mocked `/api/overview`, `/api/logs`, and `/api/resources` state before packaging. The smoke check verifies no console errors, no horizontal overflow on desktop/mobile viewports, folded setup tools by default, and larger checkbox/focus targets for form controls.
