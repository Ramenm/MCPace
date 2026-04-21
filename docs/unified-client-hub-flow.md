# Unified client hub flow

## Brief

Design a single user experience for MCPace where one local hub can serve many supported clients without making the user learn per-client rituals.

## Assumptions

- the first shipped interface should stay CLI-first
- a local background/runtime process is acceptable when it removes repeated setup work
- generated client config is preferred over asking users to hand-edit JSON repeatedly
- a richer dashboard is optional, not the first milestone

## Information architecture

### Top-level objects

- **Hub**
  - runtime status
  - logs
  - active profile
  - active sessions
- **Clients**
  - installed adapters
  - export targets
  - last generated config
- **Servers**
  - enabled / disabled
  - required / optional
  - health
  - transport kind
  - scope
- **Projects**
  - discovered roots
  - sticky session routing
  - project-local state
- **Verification**
  - doctor
  - check
  - smoke
  - readiness
  - probe

## User flow

### First-time setup

1. user installs `mcpace`
2. user runs `mcpace init`
3. user runs `mcpace client install cursor`
4. hub starts automatically or on first use
5. user opens Cursor and connects through the generated adapter
6. user can inspect status with `mcpace hub status`

### Daily use

1. user opens a project in a supported client
2. generated launcher or local HTTP endpoint connects to MCPace
3. hub resolves profile, project roots, server availability, and current health
4. hub routes to the correct upstream servers
5. user only sees MCP tools/resources/prompts, not runtime churn

### Recovery

1. user runs `mcpace doctor`
2. if needed, runs `mcpace verify check` or `mcpace repair`
3. user sees concrete blocker/fix output instead of generic “it failed”

## Three interface concepts

### Concept A — CLI-first control surface

**Idea:** one binary, subcommands only, no mandatory UI.

**Layout:** command groups by intent:
- `hub`
- `client`
- `server`
- `profile`
- `project`
- `verify`

**Key components:**
- concise table output
- `--json` machine output
- explicit exit codes
- generated client exporters

**Strengths:**
- lowest implementation risk
- easiest to automate and test
- best fit for cross-platform parity

**Risks:**
- less discoverable for non-technical users
- can still grow noisy if command taxonomy is weak

**Bad fit when:**
- the product depends on frequent visual monitoring
- users need click-first onboarding

### Concept B — Launcher-first with thin CLI

**Idea:** users mostly run `mcpace client install <client>` and then forget the CLI unless something breaks.

**Layout:** CLI is small; launchers/config exporters are the primary experience.

**Key components:**
- per-client installer/exporter
- one local endpoint
- minimal status command

**Strengths:**
- lowest day-two friction
- best fit for “just make the client work”

**Risks:**
- harder to debug if status/reporting is too hidden
- operators still need stronger lower-level commands

**Bad fit when:**
- teams need explicit operational visibility
- client capabilities differ a lot

### Concept C — Local admin panel

**Idea:** local hub exposes a browser UI or TUI for status and configuration.

**Layout:** overview, clients, servers, projects, logs, verification.

**Key components:**
- live status view
- tail logs
- quick actions
- profile/server toggles

**Strengths:**
- strongest discoverability
- easiest to demo and onboard visually

**Risks:**
- highest implementation and maintenance cost
- distracts from getting the core runtime right

**Bad fit when:**
- the main need is reliable automation
- the product still lacks command/state stability

## Recommendation

Ship **Concept A** first, keep **Concept B** as the onboarding layer, and postpone **Concept C** until the command and runtime contracts are stable.
