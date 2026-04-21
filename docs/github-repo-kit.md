# GitHub Repo Kit

## Purpose

This repository now carries a small GitHub/git hygiene kit that can be reused when a new GitHub
repository is created or when an existing repository needs to be normalized.

The kit separates three different intents:

- `prepare`: bootstrap repo structure, source-control boundaries, and verification surface
- `repair`: fix broken CI, GitHub governance drift, or source-of-truth mismatches
- `cleanup`: refactor or deslop only when someone asks for that work explicitly

That separation matters because cleanup is optional. It should not happen silently during routine
setup or repair work.

## External skill scan

On 2026-04-12, a quick `npx skills find ...` scan found nearby public skills such as:

- `autumnsgrove/groveengine@git-workflows`
- `marcfargas/skills@repo-hygiene`
- `monkey1sai/openai-cli@git-hygiene-enforcer`
- `smithery.ai@github-gh-cli`
- `akiojin/skills@gh-fix-ci`

These are useful references, but this project keeps its own local pack because the current need is
more specific: preserve a strict `prepare` / `repair` / `cleanup-on-explicit-request` contract.

## Local pack

Repo-local skills live under `skills/`:

- `skills/github-project-prepare/SKILL.md`
- `skills/github-project-repair/SKILL.md`
- `skills/github-project-cleanup/SKILL.md`

Supporting GitHub surface lives under `.github/`:

- `pull_request_template.md`
- `ISSUE_TEMPLATE/repair-report.yml`
- `ISSUE_TEMPLATE/cleanup-request.yml`

## Adaptation rules for a future repository

When applying this kit to a fresh repository:

1. Copy the `skills/` folders into the target repo or install them into `$CODEX_HOME/skills`.
2. Copy the `.github` templates and replace project-specific verification commands.
3. Add runtime, cache, and agent state paths to `.gitignore` before opening a PR.
4. Back the rules with tests so drift shows up in CI instead of in review.
5. Keep cleanup behind an explicit request or issue so maintenance work stays bounded.
