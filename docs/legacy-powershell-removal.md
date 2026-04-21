# Legacy PowerShell Removal

## What changed

All `.ps1` files were removed from the repository.

This was done after updating active docs and test cases so the repo would stop
claiming a PowerShell bridge that no longer exists.

## New guardrails

- repo-contract tests fail if any `.ps1` file returns;
- docs-contract tests fail if active docs instruct `pwsh`, `manager.ps1`, `manager.sh`, or `manager.cmd`;
- Rust source no longer shells out to a PowerShell bridge module.

## What this does not prove

It does **not** prove full runtime parity.
It proves the repository now tells the truth about its active implementation boundary.
