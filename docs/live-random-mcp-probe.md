# Live random MCP probe

MCPace has three different discovery/probe lanes:

1. `registry-lab`: metadata-only classification. It never installs or executes third-party packages.
2. `live-random-mcp-probe`: real package-manager download and stdio launch smoke for pinned npm/PyPI packages.
3. forced canaries: explicit `--force-canaries --ids ...` probes for heavy, credentialed, deprecated, or sandbox-sensitive packages. Some package-manager-heavy canaries additionally require `--allow-heavy-installs` and are otherwise hard-skipped.

The live probe intentionally performs only MCP `initialize`, `notifications/initialized`, and `tools/list`. It does not call any server tools, does not pass user API keys to runtime processes, and does not read the user's home directory. npm packages are installed with `--ignore-scripts`, `--no-audit`, `--no-fund`, `--omit=dev`, isolated HOME/cache, and a package-manager environment whitelist; PyPI packages are installed into a disposable `uv` venv with isolated cache/HOME. Package-manager mirror/proxy credentials may be used only by npm/uv so the host can reach configured registries, and package-manager stdout/stderr is redacted before it is written into reports. The install phase is also treated as untrusted supply-chain execution surface, even though package lifecycle scripts are disabled. Runtime processes receive a stripped environment and, when available, run under `unshare -Urn`.

The probe runner resolves npm/uv binaries before switching to a whitelisted PATH, validates `--ids`/`--kinds` instead of silently producing empty reports, caps captured stdout/stderr to prevent log-spam memory blowups, and hard-settles timed-out package-manager children after process-tree termination. The probe client responds to server-side `roots/list` and `ping` requests so that stdio servers that ask for roots during initialization can complete tool discovery without hanging. It rejects any other server-side request with JSON-RPC `method not found` rather than silently blocking.

Run the offline replay gate:

```bash
npm run verify:live-random-mcp-probe
```

Run the deterministic PyPI/uv live lane:

```bash
npm run verify:live-random-mcp-probe:download -- --workspace /tmp/mcpace-live-mcp-probe
```

Run the deterministic npm live lane:

```bash
npm run verify:live-random-mcp-probe:npm-stable -- --workspace /tmp/mcpace-live-mcp-probe-npm
```

Run explicit canaries, for example Chrome DevTools, Context7, code-runner, Kubernetes, Tavily, official Playwright, Google Maps, Azure, or EVM/blockchain:

```bash
node scripts/live-random-mcp-probe.mjs --download --force-canaries --kinds npm --ids chrome-devtools --json
node scripts/live-random-mcp-probe.mjs --download --force-canaries --kinds npm --ids context7 --json
node scripts/live-random-mcp-probe.mjs --download --force-canaries --kinds npm --ids code-runner --json
node scripts/live-random-mcp-probe.mjs --download --force-canaries --kinds npm --ids kubernetes-flux159 --json
node scripts/live-random-mcp-probe.mjs --download --force-canaries --kinds npm --ids tavily --json
node scripts/live-random-mcp-probe.mjs --download --force-canaries --kinds npm --ids playwright-official --json
node scripts/live-random-mcp-probe.mjs --download --force-canaries --kinds npm --ids google-maps-official --json
node scripts/live-random-mcp-probe.mjs --download --force-canaries --kinds npm --ids azure-mcp --json
node scripts/live-random-mcp-probe.mjs --download --force-canaries --kinds npm --ids evm-mcp --json
```

The consolidated evidence in `reports/live-random-mcp-probe-latest.json` covers the stable fixture set, and the addendum reports cover additional real canaries: ESLint, code-runner, Kubernetes, OpenAPI bridge, Tavily, official Playwright, Google Maps, Azure, EVM/blockchain, sanitized npm/PyPI install-env checks, and Mapbox hard-skip behavior. Together these exercises filesystem, memory/context, git, SQLite, local utility, network fetch/search, browser/devtools, project devtools, cluster-control, OpenAPI bridge, code execution, credentialed API, and protocol-fixture classes. The covered policy classes include:

- project filesystem single-writer;
- memory/context single-session;
- git repository single-writer;
- SQLite/database path single-writer;
- local utility multi-reader;
- network fetch review;
- browser host/profile lock;
- external API credential-scoped review;
- network docs multi-reader review;
- protocol fixture disabled;
- project devtools review;
- cluster-control credential review;
- OpenAPI bridge review;
- dangerous command/code runner disabled;
- cloud-admin credential review;
- blockchain/wallet review;
- payment/financial account review;
- identity-admin credential review;
- secrets-manager disabled review;
- messaging/email workspace review.

Expected behavior:

- local filesystem is project/root scoped and single-writer;
- memory and sequential-thinking are state/profile bound;
- git and SQLite are path/repository scoped;
- browser/devtools is a shared-exclusive browser profile lock;
- network fetch/documentation servers remain review-gated;
- API tools without credentials should fail closed during startup;
- protocol fixture servers are not user-install defaults;
- large or credential-heavy ecosystem packages are canaries, not default CI blockers;
- code execution servers must classify as disabled command-runners even when the only exposed tool is named generically, such as `run-code`;
- cluster-control servers must stay credential/review-gated even if `tools/list` succeeds without kubeconfig;
- cloud and blockchain/wallet servers must never be auto-enabled from successful `tools/list`;
- install probes must not inherit arbitrary user environment variables, even before runtime launch;
- package-manager logs must be redacted because mirror/proxy configuration can contain credentials;
- typoed probe ids or unsupported package kinds must fail loudly instead of producing a misleading blocked/empty report.

This lane is still not a full security audit. A later destructive/sandbox lane must use stronger filesystem isolation, package source review, tool-call fixtures, process-tree cleanup outside Node, Docker/chroot/firejail or equivalent, and concurrency torture.
