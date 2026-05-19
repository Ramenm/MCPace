# Local CI without paid GitHub runners

MCPace keeps the repository-quality gate runnable on a developer machine so a private GitHub repository does not need paid GitHub-hosted Actions minutes for every push or pull request.

## Default local gates

| Command | Purpose |
| --- | --- |
| `npm run ci:local:quick` | Fast pre-push gate: source smoke plus secret scan. |
| `npm run ci:local:source` | Mirrors the GitHub `node-source-validation` lane. |
| `npm run ci:local:package` | Mirrors the GitHub package dry-run lane. |
| `npm run ci:local:rust` | Mirrors the Rust build/lifecycle lanes. |
| `npm run ci:local` | Runs the source, package, Rust, and secrets gates in sequence. |
| `npm run ci:local:linux` | Host-only Linux auto gate; use from Linux/WSL when checking Linux behavior. |

## Pre-push hook

Install the local pre-push guard once per checkout:

```bash
npm run hooks:install
```

The hook runs `npm run ci:local:quick` before `git push`. For an emergency push after a manual full local run, skip it explicitly:

```bash
MCPACE_SKIP_LOCAL_CI=1 git push
```

On PowerShell:

```powershell
$env:MCPACE_SKIP_LOCAL_CI='1'; git push; Remove-Item Env:\MCPACE_SKIP_LOCAL_CI
```

## GitHub Actions cost posture

The repository workflows under `.github/workflows/` are manual-only (`workflow_dispatch`). This keeps GitHub-hosted runners from starting automatically on private-repo pushes, pull requests, scheduled security scans, or release tags.

Dependabot version-update PR creation is also paused with `open-pull-requests-limit: 0` in `.github/dependabot.yml` so stale dependency PRs do not sit open with obsolete failed checks while hosted CI is disabled. Re-enable Dependabot when either local dependency-update review is scheduled or a trusted CI runner is available.

If a cloud proof is needed later, run the workflow manually after GitHub billing/spending limits are healthy, or move selected jobs to a trusted self-hosted runner and keep secrets locked down.
