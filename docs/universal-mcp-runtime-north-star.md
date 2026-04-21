# Universal MCP runtime north star

`MCPace` should become a universal MCP runtime for many clients, not a bundle of
operator tricks. You connect any supported client once, and `MCPace` handles
server lifecycle, transport compatibility, local auth state, health checks,
and project-aware routing behind one stable contract.

## Product definition

`MCPace` exists to remove MCP connection pain for end users and team operators.
Its job is to make MCP servers feel boring to install, boring to connect, and
boring to keep healthy.

<!-- prettier-ignore -->
> [!IMPORTANT]
> OMX is contributor tooling, not product surface.
> End users must not need OMX, team mode, autopilot, or repo-local skills to
> use `MCPace` successfully.

## Problem statement

Today, MCP users still hit the same failure classes across clients and server
stacks:

- client-specific config shapes and launch commands;
- transport mismatches between local stdio, SSE, and Streamable HTTP;
- fragile host-specific bridges for browsers, desktop automation, and local
  processes;
- secret, OAuth, and token state spread across too many places;
- poor health visibility when a server is configured but not actually usable;
- weak multi-project ergonomics for stateful servers.

A universal runtime must absorb that complexity so the user sees one entrypoint,
clear status, and explicit fix paths.

## Who this product is for

This product serves three user groups with one runtime contract.

- **End users** need one launcher or one remote URL that works across Claude,
  Cursor, VS Code, Codex, and similar MCP hosts.
- **Team operators** need policy, health, auth bootstrap, reproducible setup,
  and safe defaults.
- **Advanced integrators** need a stable place to add custom servers, bridges,
  and project-routing logic without rewriting every client config.

## Product principles

These principles define what `MCPace` must optimize for.

1. **One client contract.** Every client should connect through one generated
   launcher, one bundle, or one remote endpoint. Per-client special casing
   must stay thin.
2. **Runtime truth over config optimism.** `configuredEnabled` is not enough.
   Users need `effectiveEnabled`, real health, and a concrete disabled reason.
3. **Client-agnostic behavior.** The product must solve runtime problems inside
   `MCPace`, not by teaching every client custom rituals.
4. **Safe defaults, explicit opt-in.** Secret-backed, OAuth-gated, and
   host-specific integrations must stay opt-in until prerequisites are proven.
5. **Project-aware isolation where needed.** Shared servers stay shared.
   Stateful project-scoped servers get isolated state.
6. **Operator-grade observability.** The runtime must tell you what is broken,
   where it broke, and what action fixes it.
7. **Portable distribution.** Installation friction must keep dropping over
   time, from launcher-first today toward bundle and remote-friendly paths.

## Product boundary

This section draws the hard line between product and contributor tooling.

### In scope for `MCPace`

`MCPace` must own the shipped runtime experience:

- launcher generation and client onboarding;
- effective settings generation;
- host bridge startup and preflight;
- Dockerized hub lifecycle;
- server install recipes and compatibility transforms;
- runtime health, diagnostics, and readiness verification;
- workspace discovery, registry, and project-aware routing.

### Out of scope for `MCPace`

These items are useful for contributors, but they are not product surface:

- OMX workflows such as `autopilot`, `team`, `ralph`, and agent orchestration;
- repo-local skills, prompts, and contributor-only automation;
- tmux worker management or multi-agent coding flows;
- contributor memory/state under `.omx/` beyond internal development.

If an end user needs OMX to use `MCPace`, product boundary is wrong.

## Approach options

There are three realistic product shapes from here.

### Option 1: Local runtime first

This path keeps `MCPace` focused on one local launcher-first endpoint.

**Strengths**

- Best fit for local files, browsers, desktop automation, and localhost tools.
- Best control over host prerequisites and local auth bootstrap.
- Works with today's current codebase and verification harness.

**Weaknesses**

- Installation still depends on local shell, Docker, and host capabilities.
- Multi-device and web-only clients stay awkward.

### Option 2: Remote runtime first

This path turns `MCPace` into a remote Streamable HTTP gateway for hosted use.

**Strengths**

- Lowest end-user setup friction.
- Best fit for cloud APIs, team-wide connectors, and web clients.
- Strong OAuth story.

**Weaknesses**

- Weak fit for local files, desktop automation, and machine-local tools.
- Requires a second story for local-only servers anyway.

### Option 3: Hybrid runtime platform

This path keeps a strong local runtime while adding clean remote and bundle
packaging lanes.

**Strengths**

- Covers both local and remote server classes.
- Lets you keep one product while supporting many client shapes.
- Matches current MCP ecosystem direction better than pure local or pure
  remote.

**Weaknesses**

- Highest architecture cost.
- Requires strict product boundaries so local runtime, bundles, and remote
  deployment do not become one tangled surface.

## Recommended direction

Choose **Option 3: hybrid runtime platform**, but stage it carefully.

The product thesis should be:

`MCPace` is universal MCP connection layer. It gives users one easy way to
connect clients, then adapts delivery mode to server class.

That means:

- local and host-bridge workloads stay local-first;
- cloud API workloads move toward remote Streamable HTTP;
- distributable local servers move toward bundles;
- clients still see one simple connection story.

## Target architecture

The long-term architecture should separate three planes.

### Client plane

The client plane is what users touch directly. It should stay tiny.

- generated launcher for local installs;
- optional bundle install surface for supported hosts;
- optional remote connector URL for remote-ready hosts;
- zero or near-zero per-client customization.

### Runtime plane

The runtime plane is the product core.

- effective settings compiler;
- transport adapters and compatibility shims;
- host bridge manager;
- hub container lifecycle manager;
- auth and token state manager;
- health and readiness evaluator;
- project registry and routing engine.

### Control plane

The control plane is optional at first, then increasingly valuable.

- server profiles such as `safe`, `extended`, and `full`;
- install recipes and compatibility metadata;
- optional remote fleet policy and telemetry;
- release and registry metadata.

## Server classification model

The runtime should classify servers by behavior, not just by transport.

### Shared global servers

These can usually stay shared across projects.

- `filesystem`
- `browser`
- `windows-mcp`
- `fetch`
- `exa`
- `sequential-thinking`

### Project-local stateful servers

These need isolated state, cwd, index, or cache.

- `serena`
- `lean-ctx`
- likely `git` once it becomes truly project-aware

### Gated optional servers

These remain opt-in until prerequisites are present.

- OAuth-gated servers
- secret-backed servers
- machine-specific bridges
- high-risk or expensive integrations

## Packaging and onboarding strategy

The onboarding story should evolve in phases instead of jumping all at once.

1. **Launcher-first now.** Keep generated launchers as the stable baseline.
2. **Profile-driven setup next.** Let users choose `safe`, `extended`, or
   `custom` without hand-editing raw config.
3. **Bundle lane after that.** Package local servers for hosts that support
   MCP bundles.
4. **Remote lane after that.** Expose remote-ready servers through one remote
   endpoint for clients that support remote MCP directly.

## Ecosystem signals

Current MCP ecosystem direction supports this hybrid view.

- MCP official architecture docs describe `stdio` for local servers and
  Streamable HTTP for remote servers, with MCP hosts creating one client per
  server.
- MCP official guidance for agent-built servers says remote Streamable HTTP is
  default for cloud APIs, while local machine-touching servers are good bundle
  candidates.
- `mcpb` exists specifically to reduce local install friction with one-click
  bundle installs in supporting hosts.
- `supergateway`, `mcp-remote`, `mcp-bridge`, and similar tools exist because
  transport mismatch is common and painful.
- `mcphub` proves there is demand for a unified orchestration layer, but its
  product center is hub management, not local universal onboarding.

<details>
<summary>Reference links</summary>

- MCP architecture overview:
  https://modelcontextprotocol.io/docs/learn/architecture
- Connect to remote MCP servers:
  https://modelcontextprotocol.io/docs/develop/connect-remote-servers
- Build with Agent Skills:
  https://modelcontextprotocol.io/docs/develop/build-with-agent-skills
- MCP Bundles (`mcpb`):
  https://github.com/modelcontextprotocol/mcpb
- Supergateway:
  https://github.com/supercorp-ai/supergateway
- MCP Remote:
  https://github.com/geelen/mcp-remote
- MCP Bridge:
  https://github.com/geosp/mcp_bridge
- upstream MCPHub project:
  https://github.com/samanhappy/mcphub
- WordPress MCP adapter:
  https://github.com/WordPress/mcp-adapter

</details>

## Near-term roadmap

The next delivery steps should narrow scope before expanding features.

1. **Lock product boundary.** Document that OMX is contributor-only and remove
   product-facing dependence on it.
2. **Stabilize required runtime path.** Keep the required server path healthy on
   Windows first, then prove Ubuntu and macOS.
3. **Add setup profiles.** Replace ad hoc local overrides with clear install
   intents and profile semantics.
4. **Build project registry v1.** Discovery roots, project registry, sticky
   session binding, and safe ambiguity handling.
5. **Isolate stateful servers.** Give `serena` and `lean-ctx` project-local
   state and lifecycle.
6. **Design packaging v2.** Evaluate launcher, bundle, and remote surfaces as
   first-class distribution modes under one product name.

## Non-goals for next phase

The next phase must stay disciplined.

- Do not build full hosted multi-tenant SaaS first.
- Do not expose twenty separate client endpoints.
- Do not solve every optional integration before the required path is boring.
- Do not leak contributor workflow machinery into user onboarding.

## Next steps

You can use this document as the product north star for follow-up work.

1. Rewrite README positioning so end-user story centers on universal client
   onboarding, not repository internals.
2. Convert current routing RFC into an implementation PRD with milestones.
3. Add profile and packaging RFCs.
4. Add platform-proof verification lanes for Windows, Ubuntu, and macOS.
