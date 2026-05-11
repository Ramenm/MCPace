# ADR 0005: Cargo CI caching and upstream stderr diagnostic redaction

## Status

Accepted for the current source/release pipeline and stdio upstream bridge.

## Context

MCPace runs Rust quality gates and platform release builds across hosted GitHub Actions runners. These jobs use the pinned Rust `1.95.0` toolchain and `Cargo.lock`, but previously had no Cargo cache in the Rust jobs. MCPace also surfaces selected upstream stderr lines when a stdio MCP server times out, exits early, or returns malformed responses. MCP permits servers to write UTF-8 logging to stderr, but user-supplied servers can accidentally print tokens, Authorization headers, or other credentials.

## Problem / goal

Make the CI pipeline less wasteful and less sensitive to transient dependency downloads without changing release semantics. Keep stderr diagnostics useful for debugging while preventing them from becoming a secret-leak path.

## Constraints and non-goals

- Do not introduce a new package manager or global install path.
- Do not cache `node_modules`; the repo has no checked-in npm lockfile today.
- Do not hide all upstream stderr, because it is often the only actionable startup/timeout clue.
- Do not claim the local sandbox completed the full Rust quality gate when network/toolchain resolution blocks it.

## Considered options

1. **No CI cache and raw stderr diagnostics.** Lowest implementation effort, but repeated Rust downloads/builds remain slow and raw diagnostics may leak secrets.
2. **Use official `actions/cache@v4` for Cargo registry/git/target plus bounded stderr redaction.** Chosen. It follows GitHub's documented Rust cache shape, keeps keys deterministic with OS, Rust version, target/suite, `Cargo.lock`, and `rust-toolchain.toml`, and preserves useful diagnostics after sanitization.
3. **Use a third-party Rust cache action and suppress all stderr.** Potentially convenient, but adds a new dependency and harms operability by removing safe diagnostic context.

## Selected solution

- Add `actions/cache@v4` to Rust quality, lifecycle, launcher-smoke, dry-run, and release native jobs.
- Key caches by runner OS, Rust version, target or suite, `Cargo.lock`, and `rust-toolchain.toml`; use narrower restore keys for safe partial reuse.
- Set `persist-credentials: false` on checkout steps that do not need authenticated git commands.
- Sanitize upstream stderr before adding it to user-visible error messages:
  - redact likely token/password/credential/API-key/private-key/Authorization assignments;
  - redact bearer tokens;
  - preserve safe surrounding context;
  - bound diagnostic lines and per-line length.

## Consequences / risks

- Cargo target caches may grow and should be monitored through GitHub cache usage limits.
- The redactor is heuristic; it reduces common leakage risk but is not a formal DLP engine.
- Some sensitive values with unusual formatting can still evade redaction, so upstream stderr must remain bounded and treated as diagnostic-only.
- False positives may redact some harmless values with sensitive-looking key names.

## Plan / verification

- Node contract tests assert CI cache wiring and checkout credential minimization.
- Node security contract tests assert that upstream stderr uses sanitizer functions and Rust regression tests exist.
- Rust unit tests cover redaction, line count, and line length behavior.
- Full Rust fmt/clippy/test/build still needs the pinned Rust toolchain and dependency network/cache.

## Open questions

- Whether to add a checked-in npm lockfile later. If one is added, `actions/setup-node` npm cache can be considered; without a lockfile, caching npm is intentionally not added.
- Whether future telemetry should emit structured error categories for upstream failures instead of only sanitized stderr suffixes.
