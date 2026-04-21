# Runtime Lab

The runtime lab answers a simple question:

**What is already covered by the current Rust read-path surface, what is only partially covered, and what is still blocked until the live hub exists?**

## Files

- `eval/runtime-capabilities.json` — capability inventory with status, priority, evidence, and next step
- `eval/fixtures/runtime/*.json` — production-like runtime scenarios
- `src/lab.rs` — grouped CLI surface for reading the lab
- `src/client_catalog.rs` — surface-aware client catalog used by lab coverage and gap reports

## Commands

```bash
mcpace lab list
mcpace lab matrix
mcpace lab coverage
mcpace lab gaps
mcpace lab report
mcpace lab show --id <scenario>
```

## Why this exists

The project can already inspect and plan, but it does not yet run a live hub. The lab keeps those layers separate and now also tracks **which client surface** each scenario targets:

- **covered now** — scenarios the planner or read-path surface already supports
- **partial** — scenarios where some ingredients exist but a runtime or adapter gap remains
- **blocked** — scenarios that depend on still-missing runtime, adapter, or compatibility work

## Fixture shape

Each runtime scenario contains:

- `id`
- `suite`
- `category` (`typical`, `edge`, `adversarial`, `held-out`)
- `proofLayer` (`planner`, `runtime`, `adapter`, `compat`, `release`)
- `title`
- `objective`
- `traffic.clientArchetype` — ideally one id from the surface-aware client catalog
- `traffic.serverPolicies`
- `traffic.signals`
- `checks`
- `requires` — capability ids from `eval/runtime-capabilities.json`

## Capability inventory shape

Each capability entry contains:

- `id`
- `area`
- `title`
- `status` (`implemented`, `planned`, or `missing`)
- `priority` (`p0`, `p1`, `p2`)
- `summary`
- `evidence`
- `nextStep`


## Surface-aware reporting

`mcpace lab coverage` now summarizes not only scenario ids and signals, but also:

- client families
- surface classes (`local`, `cloud`, `generic`)
- surface kinds (`local-cli`, `cloud-agent`, `cloud-api-connector`, etc.)
- documented constraints seen in the fixture set (`tools-only`, `public-http-only`, `tool-budget-100`, ... )

This keeps local/cloud divergence visible in the backlog instead of hiding it under one brand name.
