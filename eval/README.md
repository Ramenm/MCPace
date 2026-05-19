# Eval fixtures and governance

This directory now holds both the **runtime lab** and the **prompt/agent eval governance** used to keep MCPace honest.

## What lives here

### 1. Seed prompt / agent evals

`eval/fixtures/seed/` contains repo-grounded cases for planning, proof claims, project-status reporting, and anti-overclaim behavior.

Each seed case now carries:

- `track`
- `bucket`
- `split` and `heldOut`
- grounding evidence paths
- expected good / bad / unacceptable behavior
- scoring methods and binary checks
- metrics and failure mode

### 2. Runtime lab cases

`eval/fixtures/runtime/` contains production-like runtime scenarios, and `eval/runtime-capabilities.json` maps those scenarios to concrete capabilities. The capability inventory now keeps both implementation status (`status`) and the strongest honest public claim (`claimStatus`) so roadmap/docs can distinguish supported, control-plane-only, bootstrap-only, connectable-preview, and still-planned slices.

### 3. Eval governance files

- `eval/scenario-matrix.json` — which request families matter, how often they matter, and which fixtures cover them
- `eval/scoring-rubric.json` — good / bad / unacceptable answer definitions and metric policy
- `eval/dataset-plan.json` — dataset splits, source mix, and regression loop

## Runtime lab commands

```bash
mcpace lab list
mcpace lab matrix
mcpace lab coverage
mcpace lab gaps
mcpace lab report
mcpace lab show --id <scenario>
```

## Why this changed

The previous eval surface was directionally useful, but too easy to treat as a nice-looking fixture set.

The current goal is stricter:

- production-like maintainer tasks over generic benchmarks
- historical regressions over pretty demos
- split metrics over one vanity number
- held-out cases that are not part of day-to-day tuning
- honest abstention over confident guessing

### 4. Autonomous-agent workloop evals

The seed set now includes raw maintainer prompts that ask the assistant to recover state, choose a reversible next step, execute, verify, and report facts separately from inferences and blockers. These cases prevent the agent from treating broad execution requests as generic planning or from turning “do everything” pressure into unsafe random-server execution or release overclaims.
