# Test Strategy

## Principle

Treat source, build, runtime, and release as separate proof gates.
Passing one does not grant the others.

## 1. Source checks

Run in this repo today:

- Node repo-contract tests
- npm launcher unit tests
- manifest/schema/example validation
- docs/contract drift checks
- runtime fixture and capability-inventory contract checks
- seed prompt/agent eval contract checks
- evidence-path checks for runtime capabilities and seed evals
- eval scenario-map / rubric / dataset-plan validation
- stack-policy drift checks across `package.json`, `.nvmrc`, `.node-version`, CI, and docs
- machine-generated verification report contract checks for `scripts/proof-report.mjs`

## 2. Build checks

Need a host with a real Rust toolchain:

- `cargo test`
- `cargo build --release`
- later `cargo nextest run`

## 3. Runtime checks

Read-path runtime-lab checks now exist in-source, while real transport/runtime proof still requires supported hosts:

- `mcpace doctor`
- `mcpace client list`
- `mcpace client plan`
- `mcpace lab list` / `matrix` / `coverage` / `gaps` / `report` / `show`
- `mcpace server list`
- `mcpace verify doctor`
- `mcpace verify readiness`
- `cargo test --test hub_runtime hub_up_releases_captured_stdio_for_background_launcher -- --exact`
- `node scripts/verify-ubuntu-docker-fast.mjs --json`
- `node scripts/verify-ubuntu-docker-e2e.mjs --json`
- `node scripts/verify-ubuntu-docker-full.mjs --json`
- later local `stdio` transport
- later local `Streamable HTTP`
- later Docker-backed runtime prerequisites
- later supported client install/export flows

## 4. Release checks

Run in CI before publication:

- artifact manifest validation
- release bundle validation
- vendored binary bundle validation when a host binary is staged
- npm publish dry-run and provenance checks
- cross-host Rust build/test matrix

## Active suites in this repo

- `tests/node/repo-contract.test.js`
- `tests/node/docs-contract.test.js`
- `tests/node/schema-examples.test.js`
- `tests/node/fixtures-contract.test.js`
- `tests/node/stack-contract.test.js`
- `tests/node/evidence-contract.test.js`
- `tests/node/eval-contract.test.js`
- `packages/npm/cli/test/*.test.mjs`
- `tests/help_and_root.rs`
- `tests/hub_runtime.rs`
- `tests/config_and_server.rs`
- `tests/client_surface.rs`
- `tests/lab_surface.rs`

## CI shape

- Ubuntu carries both maintained Node LTS lines for source proof;
- Windows and macOS validate the default local Node line;
- Windows and macOS also run a targeted launcher smoke test on real hosted runners;
- Ubuntu runs the same launcher smoke through a fast Docker lane that copies only the
  Rust workspace slice into the container instead of bind-mounting the full repo;
- Ubuntu also runs a constrained Docker E2E smoke lane with CPU, memory, and pid
  limits so launcher/runtime behavior is checked under bounded resources;
- Ubuntu also runs a constrained Docker full-work lane that builds the release
  binary inside a Rust+Node verify image and exercises the repo-root CLI path;
- npm package dry-run is separated into its own job to reduce duplicate work;
- Rust build proof remains a three-host matrix and is still the required build gate.

## Fast launcher verification lanes

Use a small launcher proof before the full Rust matrix when you only need to check
background hub startup semantics.

- **Windows host:** run
  `cargo test --test hub_runtime hub_up_releases_captured_stdio_for_background_launcher -- --exact`
- **Ubuntu in Docker:** run `node scripts/verify-ubuntu-docker-fast.mjs --json`
- **Ubuntu E2E in Docker:** run `node scripts/verify-ubuntu-docker-e2e.mjs --json`
- **Ubuntu full-work in Docker:** run
  `node scripts/verify-ubuntu-docker-full.mjs --json`
- **macOS:** run the same targeted Rust test on a real `macos-latest` runner or real
  Apple hardware. Do not treat emulators as release-grade evidence for process-launch
  behavior.

The Ubuntu Docker E2E lane now exercises a wider lifecycle slice:

- corrupt runtime state
- `hub status`
- `hub repair`
- `hub up`
- `hub logs`
- `hub down`

The Ubuntu Docker full-work lane extends that proof with the repo-root checks
that the Rust-only image cannot cover on its own:

- `mcpace version`
- `mcpace doctor`
- `mcpace client list`
- `mcpace client plan`
- `mcpace server list`
- `mcpace verify doctor`
- `mcpace verify readiness`
- corrupt-state recovery through top-level `mcpace repair`
- `hub up` / `hub logs` / `hub down`

## Resource-bounded Docker preference

For Linux verification, prefer Docker lanes with explicit limits over host execution:

- `--cpus 1.0`
- `--memory 768m`
- `--pids-limit 256`

Those limits are the default for the Ubuntu Docker launcher scripts in this repo.
If a future lane needs higher limits, raise them deliberately and document why.

## Failure classes we explicitly guard now

- client metadata resolves to the wrong project or session;
- project-local servers are shared without an explicit project root;
- `single-session` or `single-writer` servers are treated as parallel-safe;
- runtime fixtures drift away from the capability inventory or the reported backlog;
- docs claim full client onboarding when only `client plan` exists;
- build/runtime proof is claimed from Node/source checks alone;
- project status or ETA is reported with fake precision;
- eval changes improve a vanity score while hiding unsupported-claim regressions.
