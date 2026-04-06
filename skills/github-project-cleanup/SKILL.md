---
name: github-project-cleanup
description: Use when the user explicitly asks for repository cleanup, deslop, refactor, or hygiene work in a GitHub project. Apply only after the request is explicit and bounded, especially when git structure, docs, workflows, or ownership files need cleanup without changing intended behavior.
---

# GitHub Project Cleanup

Cleanup is opt-in maintenance. Do not run it implicitly.

## Entry gate

- Start only when the user explicitly asks for cleanup, deslop, or refactor work.
- If the request is ambiguous, stop and restate the cleanup scope before editing.
- Normal prepare or repair work does not authorize cleanup.

## Cleanup rules

- Write a cleanup plan before touching files.
- Lock existing behavior with tests or contract checks before structural edits.
- Prefer deletion, consolidation, and clearer ownership over new layers.
- Keep GitHub surface changes bounded to the approved cleanup scope.

## Do not

- hide cleanup inside a repair task
- widen a preparation task into aesthetic or architectural cleanup
- change behavior unless the cleanup request explicitly includes behavior changes

## Verification

- run the tests or checks that locked the current behavior
- run the repo verify path after cleanup
- list what stayed intentionally untouched
