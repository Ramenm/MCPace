# Architecture simplification guardrails

This pass keeps the hardening behavior from the previous releases, but moves repeated proof/preflight mechanics into one small shared layer.

## Shared command execution

`local-proof` and `tooling-preflight` both need the same answers:

- whether a tool name is a safe executable token;
- whether the tool exists on `PATH` without invoking a shell probe;
- how to use the local `node_modules/.bin` command when that is intended;
- how to spawn the command with the same cleaned child environment.

Those rules now live in `scripts/lib/command-runner.mjs`. Callers can still decide policy (`required`, `optional`, `warn`), but they do not reimplement path probing or Windows shim handling.

## Atomic report output

Generated proof reports should not have a separate write policy per report script. `project-assurance`, `platform-proof`, and `project-inventory` now use the same atomic writer as the release builder and local proof reports.

## Release invariant

Any Node script imported by release-time automation must either be included as a specific `release-manifest.json` entry or live under an included directory. This prevents a source ZIP that passes locally but lacks a shared helper after extraction.

## What this deliberately does not do

This is not a behavior rewrite. The goal is to reduce duplicated automation mechanics while preserving existing CLI, release, package, and MCP runtime behavior.
