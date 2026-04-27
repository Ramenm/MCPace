# Tombstones

This file tracks removed or intentionally excluded surfaces so release/recovery work does not quietly resurrect them.

## Removed from the active repo contract

- PowerShell entrypoints and bridge wrappers
- shell-based manager launch surfaces
- any expectation that npm or skills are a second runtime core

## Intentionally excluded from source archives

- `.git`
- `node_modules`
- `target`
- runtime state directories and caches
- OS junk such as `.DS_Store`, `Thumbs.db`, `__MACOSX`

## Intentionally still not claimed as done

- top-level grouped `release`
- local `stdio` ingress
- local Streamable HTTP ingress
- exclusive lease enforcement
- cancel/stale-result runtime guards
- config-writing `client export` for blocked/public surfaces
- real-host runtime proof
- published npm provenance proof
