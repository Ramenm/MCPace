# Rust Rewrite Architecture

## Current state

The repository now has one active implementation direction:

- Rust CLI/runtime source under `src/`
- npm launcher wrapper under `packages/npm/cli`
- no active PowerShell control plane

## Current implemented surface

- `version`
- `doctor`
- `profile show --json`
- `projects list --json`
- `candidates --json`
- `client plan`
- `server list`
- `server capabilities`
- `server candidates`
- `verify doctor`
- `verify readiness`

## Still planned command groups

- `init`
- `hub`
- `client install/export`
- `repair`
- `release`

## Architectural rule

Do not translate the old many-script surface one-for-one.
Consolidate to a smaller grouped CLI and keep unsupported surfaces explicit.

## Client/hub rule

The future hub owns:

- client/session/project routing;
- upstream process ownership for stdio servers;
- request serialization for non-parallel servers;
- one entry-point contract for many clients.
