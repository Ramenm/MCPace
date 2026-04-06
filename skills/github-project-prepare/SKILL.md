---
name: github-project-prepare
description: Use when starting a new GitHub repository, importing an existing project into GitHub, or tightening repository governance before normal development. Apply when the repo needs source-control boundaries, ignore rules for generated state, contributor surface, PR or issue templates, CI entrypoints, or verification-first setup.
---

# GitHub Project Prepare

Prepare the repository for normal work without changing product behavior.

## Core rules

- Inventory source of truth, generated state, local runtime state, and existing verification commands first.
- Tighten `.gitignore` before adding new automation so generated files, caches, auth state, and agent state do not leak into git.
- Reuse real project commands for verification; do not invent CI steps the repository cannot run locally.
- Prefer small governance files that encode behavior in tests, templates, and docs.

## Expected surface

- repo basics are present and current: ignore rules, contributing guidance, ownership, and runtime boundaries
- GitHub entrypoints exist when they reflect reality: workflows, PR template, issue forms, labels or docs
- verification commands are discoverable and can be run before review
- generated or disposable state is documented as non-source data

## Do not

- treat cleanup as part of normal preparation
- do runtime repair here when a specific failure is already known; use the repair skill instead
- add speculative automation, release stages, or platform claims without evidence

## Verification

- run the repo governance tests first and again after changes
- run the repo's real verify or test entrypoints
- report any remaining gaps as `Not-tested`
