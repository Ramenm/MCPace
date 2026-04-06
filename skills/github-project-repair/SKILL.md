---
name: github-project-repair
description: Use when a GitHub repository already exists but CI, workflow wiring, repo governance, verification entrypoints, or git boundaries are broken or have drifted. Apply when you need to reproduce a failure, repair the smallest viable surface, and re-verify it without turning the task into broad cleanup.
---

# GitHub Project Repair

Repair broken repository behavior with the smallest defensible diff.

## Core rules

- Reproduce the failure first from a command, test, workflow log, or concrete GitHub artifact.
- Identify whether the break is in source of truth, generated state, or GitHub surface, and fix the right layer.
- Keep the patch minimal; repair should not turn into a broad refactor.
- Preserve prepare-time rules such as ignored runtime state, contributor guidance, and verification entrypoints.

## Repair sequence

1. Reproduce the failing path.
2. Add or update the smallest regression test or contract check that exposes it.
3. Apply the minimal fix in source.
4. Re-run the failing command and the relevant verification suite.
5. Document residual risk instead of hiding it inside "cleanup."

## Do not

- use repair to justify repo-wide cleanup
- mutate generated artifacts when the real fix belongs in source files
- expand scope unless the original failure proves a wider contract hole

## Verification

- re-run the reproduced failure
- run the nearest repo verify path after the minimal fix
- mention any `Not-tested` area explicitly
