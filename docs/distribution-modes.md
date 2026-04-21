# Distribution modes

Distribution modes describe how end users and operators receive the `MCPace`
runtime without changing the core product promise.

That promise stays simple: connect a client once, then let `MCPace` handle the
runtime complexity behind a stable contract.

## Why distribution modes matter

Not every MCP server belongs in the same delivery shape.

Some servers need local machine access. Some are natural remote services. Some
can be packaged for one-click local install in hosts that support MCP bundles.
A universal runtime must choose the right mode per server class instead of
forcing one transport and one install story onto everything.

## Mode 1: Launcher-first local runtime

Launcher-first local runtime is the current shipped baseline.

### Best fit

- local files and local repositories;
- browser automation and other host bridges;
- desktop automation;
- mixed stacks where the runtime must coordinate host and container behavior.

### Strengths

- strongest control over local prerequisites;
- best visibility into host bridge state;
- works with today's current verification harness.

### Weaknesses

- highest local installation friction;
- still depends on local shell, Docker, and host capabilities.

## Mode 2: Bundle-first local distribution

Bundle-first local distribution packages the runtime or selected local servers
for MCP hosts that support installable bundles.

### Best fit

- users who want simpler local install than raw shell scripts;
- local-only tools that do not need a full operator workflow;
- hosts that support `mcpb` or a similar local package flow.

### Strengths

- lower setup friction than pure launcher-first flows;
- better fit for host ecosystems that already understand bundle installs.

### Weaknesses

- bundle support depends on the client host;
- host-bridge and Docker-heavy workflows may still need launcher support.

## Mode 3: Remote runtime

Remote runtime exposes a stable remote MCP endpoint for cloud-first connectors
and managed operation.

### Best fit

- cloud APIs;
- team-wide shared connectors;
- hosted or centrally operated deployments.

### Strengths

- lowest end-user machine setup cost;
- best fit for remote auth and managed policy.

### Weaknesses

- weak fit for local files and host-only automation;
- still needs a local companion story for local-only servers.

## Recommended product stance

`MCPace` should not pick only one mode forever.

Recommended stance:

- keep launcher-first local runtime as the reliable baseline now;
- add bundle support where it reduces local friction safely;
- add remote runtime where server class naturally belongs there.

This keeps one product while letting server class drive delivery shape.

## Server-class mapping

Distribution mode should follow server behavior.

### Local-first candidates

- `browser`
- `windows-mcp`
- `filesystem`
- other machine-touching tools

### Bundle-friendly candidates

- local tools with clean local dependencies;
- tools that do not need large operator workflows;
- tool groups that a bundle-capable MCP host can install predictably.

### Remote-first candidates

- cloud APIs;
- OAuth-heavy remote services;
- shared team connectors.

## Operator guidance

Operators should choose the simplest mode that matches the server class.

Use launcher-first when the runtime must coordinate host and container state.
Use bundle-first when client-host packaging can replace shell friction safely.
Use remote runtime when the integration is already remote by nature.

## Next steps

The next architecture step is to define a packaging RFC that maps concrete
server groups to launcher, bundle, and remote modes with verification gates for
all three.
