#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { writeFileAtomicSync } from './lib/atomic-fs.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, '..');
const args = new Set(process.argv.slice(2));
const write = args.has('--write');
const check = args.has('--check');
const jsonOnly = args.has('--json');

function posix(relativePath) {
  return relativePath.split(path.sep).join('/');
}

function exists(relativePath) {
  return fs.existsSync(path.join(repoRoot, relativePath));
}

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

function readJson(relativePath) {
  return JSON.parse(read(relativePath));
}

function all(...checks) {
  return checks.every(Boolean);
}

function any(...checks) {
  return checks.some(Boolean);
}

function claim({ id, title, status, why, evidence = [], failureMode, nextCheck }) {
  return { id, title, status, why, evidence, failureMode, nextCheck };
}

function statusOrder(status) {
  return ({ fail: 0, warn: 1, unverified: 2, pass: 3 }[status] ?? -1);
}

function worstStatus(claims) {
  return claims.map((item) => item.status).sort((a, b) => statusOrder(a) - statusOrder(b))[0] ?? 'unverified';
}

function dashboardRouteCount() {
  const source = read('src/dashboard.rs');
  return [...source.matchAll(/\("(GET|POST|DELETE)",\s*"([^"]+)"\)/g)].length;
}

function buildAssurance() {
  const mcpSettings = readJson('mcp_settings.json');
  const packageJson = readJson('package.json');
  const manifest = readJson('release-manifest.json');
  const dashboardHtml = read('src/dashboard/index.html');
  const dashboardCss = read('src/dashboard/frontend/styles.css');
  const dashboardApp = read('src/dashboard/frontend/app.js');
  const dashboardFrontend = `${dashboardHtml}\n${dashboardCss}\n${dashboardApp}`;
  const dashboardBackend = read('src/dashboard.rs');
  const overview = read('src/dashboard/overview.rs');
  const httpBoundary = read('src/dashboard/http_boundary.rs');
  const httpSession = read('src/dashboard/http_session.rs');
  const mcpHttp = read('src/dashboard/mcp_http.rs');
  const response = read('src/dashboard/response.rs');
  const dashboardTest = read('tests/node/dashboard-contract.test.mjs');
  const releaseTest = read('tests/node/docs-and-package.test.mjs');
  const serverModel = read('src/server/model.rs');
  const packageScripts = packageJson.scripts ?? {};
  const platformProof = exists('reports/platform-proof.json') ? readJson('reports/platform-proof.json') : null;
  const platformWorkflow = exists('.github/workflows/platform-proof.yml') ? read('.github/workflows/platform-proof.yml') : '';
  const cargoToml = read('Cargo.toml');

  const claims = [
    claim({
      id: 'safe-empty-default',
      title: 'Fresh bundle starts with no silently enabled upstream server',
      status: Object.keys(mcpSettings.mcpServers ?? {}).length === 0 ? 'pass' : 'fail',
      why: 'A user should not accidentally expose filesystem, memory, browser, or shell tools just by installing MCPace.',
      evidence: ['mcp_settings.json:mcpServers', 'README.md:does not add a filesystem server'],
      failureMode: 'Silent default tools would make the dashboard look useful while hiding a high-risk permission grant.',
      nextCheck: 'Install one explicit server, verify it stays disabled by default, then run Test manually.',
    }),
    claim({
      id: 'user-readiness',
      title: 'Dashboard starts with a backend-owned foundation verdict, not raw internals',
      status: all(overview.includes('mcpace.dashboardFoundation.v1'), overview.includes('build_dashboard_foundation_json'),
        dashboardHtml.includes('id="base-setup"'), dashboardFrontend.includes('renderBaseSetup'),
        dashboardFrontend.includes('buildFoundationModelFromOverview(overview.dashboardFoundation)')) ? 'pass' : 'fail',
      why: 'The first screen should answer the base setup path: backend, client, source, tools, and routing.',
      evidence: ['src/dashboard/overview.rs:dashboardFoundation', 'src/dashboard/frontend/app.js:renderBaseSetup'],
      failureMode: 'Users see many facts but cannot decide what basic setup step to complete next.',
      nextCheck: 'Open dashboard after mcpace up and confirm the base foundation changes when no servers, parked servers, unchecked servers, and ready servers are present.',
    }),
    claim({
      id: 'operator-plan',
      title: 'Backend produces a per-server operator plan and runbook',
      status: all(overview.includes('mcpace.operatorPlan.v1'), overview.includes('build_operator_plan_json'),
        overview.includes('operator_commands'), dashboardFrontend.includes('renderServerRunbook')) ? 'pass' : 'fail',
      why: 'Per-server UI should be driven by backend evidence: lane, blocker, next action, and commands to run.',
      evidence: ['src/dashboard/overview.rs:operatorPlan', 'src/dashboard/frontend/app.js:renderServerRunbook'],
      failureMode: 'Frontend invents readiness from partial fields and drifts away from backend runtime truth.',
      nextCheck: 'Compare each server row with mcpace server capabilities <name> --json and verify the same launch/evidence/policy direction.',
    }),
    claim({
      id: 'runtime-control-plane',
      title: 'Runtime control combines live evidence, risk, parallelism, isolation, and resource budgets',
      status: all(overview.includes('mcpace.runtimeControlPlane.v1'), overview.includes('toolRisk'), overview.includes('parallelism'), overview.includes('isolation'), overview.includes('resourceBudget'), overview.includes('mcpace.serverResourceMonitoring.v1'), dashboardFrontend.includes('renderRuntimeControl')) ? 'pass' : 'fail',
      why: 'A useful MCP runtime manager must not stop at server lists; it should explain when to probe, serialize, approve, restrict, or sandbox each server.',
      evidence: ['src/dashboard/overview.rs:runtimeControlPlane', 'src/dashboard/overview.rs:serverResourceMonitoring', 'src/dashboard/frontend/app.js:renderRuntimeControl'],
      failureMode: 'The UI claims readiness without connecting tool evidence, risk, concurrency, isolation, and observed resource use.',
      nextCheck: 'Run a harmless upstream_call and verify /api/resources reports a live per-server session row with pid/resource data.',
    }),
    claim({
      id: 'frontend-backend-contract',
      title: 'Dashboard frontend calls are covered by backend routes and DOM contract tests',
      status: all(dashboardTest.includes('frontend references only backend routes'), dashboardTest.includes('element registry only points'),
        dashboardTest.includes('action payload contract')) ? 'pass' : 'fail',
      why: 'UI can be pretty but useless if buttons, routes, payload keys, or DOM ids drift independently.',
      evidence: ['tests/node/dashboard-contract.test.mjs', `src/dashboard.rs:${dashboardRouteCount()} static routes`],
      failureMode: 'A user clicks Enable/Test/Add server and nothing real happens, or an action reaches backend with the wrong JSON.',
      nextCheck: 'Run npm run check before every release and add a new assertion for every new UI action.',
    }),
    claim({
      id: 'server-launch-visible',
      title: 'Each server exposes how it starts without leaking secrets',
      status: all(serverModel.includes('sourceCommand'), serverModel.includes('sourceArgs'), serverModel.includes('sourceEnvNames'),
        serverModel.includes('sourceHeaderNames'), overview.includes('launch_command_for_server'), dashboardFrontend.includes('sourceEnvNames')) ? 'pass' : 'fail',
      why: 'A user must see command/URL, but only names of env/header keys, never secret values.',
      evidence: ['src/server/model.rs:sourceCommand/sourceArgs/sourceEnvNames/sourceHeaderNames', 'src/dashboard/overview.rs:launch_command_for_server'],
      failureMode: 'The dashboard either cannot explain what will run, or it leaks tokens/API keys.',
      nextCheck: 'Create a server with env/header config and verify dashboard shows key names only.',
    }),
    claim({
      id: 'add-server-preflight',
      title: 'Adding a server by command is preflighted and does not execute shell compositions',
      status: all(dashboardBackend.includes('command_line_uses_shell_composition'), dashboardFrontend.includes('commandLineLooksComposed'),
        dashboardBackend.includes('server install commandLine cannot contain control characters or newlines'),
        dashboardFrontend.includes('postServerAction("server-install-command"')) ? 'pass' : 'fail',
      why: 'Paste-a-command UX is useful only if it is a source-record workflow, not a hidden arbitrary shell execution path.',
      evidence: ['src/dashboard.rs:write_server_install_command_action', 'src/dashboard/frontend/app.js:commandLineLooksComposed'],
      failureMode: 'A malicious or accidental command line can chain extra operations through ;, &&, pipes, redirects, backticks, or command substitution.',
      nextCheck: 'Try npx/uvx/url/local-path examples and then try blocked shell-composition strings; all blocked strings should be rejected before install.',
    }),
    claim({
      id: 'http-boundary',
      title: 'Local Streamable HTTP boundary has explicit origin/header/session checks',
      status: all(httpBoundary.includes('is_allowed_local_authority'), httpBoundary.includes('localhost'),
        mcpHttp.includes('missing required Accept header entries'), mcpHttp.includes('missing required Content-Type header'),
        httpSession.includes('MCP-Protocol-Version'), response.includes('X-Frame-Options')) ? 'pass' : 'fail',
      why: 'Local HTTP MCP endpoints need browser-facing hardening: origin/host checks, expected content types, session binding, and hardened dashboard responses.',
      evidence: ['src/dashboard/http_boundary.rs', 'src/dashboard/mcp_http.rs', 'src/dashboard/http_session.rs', 'src/dashboard/response.rs'],
      failureMode: 'Browser-origin or DNS-rebinding style access can reach a local MCP endpoint without the user intending it.',
      nextCheck: 'Run negative HTTP tests for bad Origin, bad Host, missing Accept, missing Content-Type, reused request id, and stale session id.',
    }),
    claim({
      id: 'human-in-loop-tools',
      title: 'Tools are not treated as trusted until visible evidence and an explicit operator action exist',
      status: all(overview.includes('tools/list evidence'), overview.includes('no tools/list evidence is assumed'),
        overview.includes('Run Test to collect initialize and tools/list evidence'), dashboardFrontend.includes('Test')) ? 'pass' : 'fail',
      why: 'MCP tools expose external actions; the UI should make tool exposure and evidence visible instead of quietly trusting package names.',
      evidence: ['src/dashboard/overview.rs:operator_plan_for_server', 'src/dashboard/frontend/app.js:server-test'],
      failureMode: 'The user thinks a server is ready just because it is configured, even though no live initialize/tools-list proof exists.',
      nextCheck: 'For a new server: add disabled, enable intentionally, run Test once, compare tool names/count with server capabilities JSON.',
    }),
    claim({
      id: 'release-reproducibility',
      title: 'Release artifact is manifest-driven and excludes heavy/runtime directories',
      status: all(manifest.includePaths.includes('tests/node'), manifest.includePaths.includes('scripts/build-release-artifacts.mjs'),
        releaseTest.includes('source bundle excludes'), releaseTest.includes('release manifest matches')) ? 'pass' : 'fail',
      why: 'A reliable project review needs a deterministic source bundle with tests, scripts, schemas, docs, and no node_modules/.git/target/dist cargo.',
      evidence: ['release-manifest.json', 'scripts/build-release-artifacts.mjs', 'tests/node/docs-and-package.test.mjs'],
      failureMode: 'The archive looks complete but omits checks or includes local build/cache state that hides reproducibility issues.',
      nextCheck: 'Run npm run build:release-artifacts and verify zipVerification.status is pass.',
    }),
    claim({
      id: 'platform-proof-matrix',
      title: 'Linux, macOS, and Windows have an explicit proof matrix',
      status: all(platformProof?.overall === 'pass', platformWorkflow.includes('macos-latest'), platformWorkflow.includes('windows-latest'), platformWorkflow.includes('ubuntu-latest'), packageScripts['check:platform'], packageScripts['platform:binary-smoke']) ? 'pass' : 'fail',
      why: 'Cross-platform readiness cannot be inferred from a Linux-only source review; each desktop OS needs Node checks, Rust build/test, and native binary smoke commands.',
      evidence: ['.github/workflows/platform-proof.yml', 'reports/platform-proof.json', 'scripts/platform-binary-smoke.mjs'],
      failureMode: 'The project appears portable while Windows/macOS path, shell, process, service, or binary resolution code is untested.',
      nextCheck: 'Run the manual platform-proof workflow with full=true and keep reports/platform-proof.json green.',
    }),
    claim({
      id: 'console-ui-scope',
      title: 'Console UI scope avoids pulling Tauri before platform proof is green',
      status: all(platformProof?.uiDecision?.decision?.includes('Tauri'), platformProof?.uiDecision?.nextTuiGate?.includes('Ratatui'), !/\btauri\b/i.test(cargoToml)) ? 'pass' : 'fail',
      why: 'Tauri is the right family for a packaged desktop webview, not a terminal UI. The safer console path is to reuse backend readiness/operatorPlan in a future Ratatui TUI after native platform proof passes.',
      evidence: ['reports/platform-proof.md:Console UI decision', 'Cargo.toml'],
      failureMode: 'A heavy desktop shell duplicates dashboard logic before the native CLI/runtime has been proven on Linux/macOS/Windows.',
      nextCheck: 'After platform-proof is green on all OS families, add `mcpace tui` as a thin terminal view over userReadiness/operatorPlan.',
    }),
    claim({
      id: 'rust-runtime-unverified-here',
      title: 'Rust compile/clippy/test pass remains a required live gate',
      status: 'warn',
      why: 'Static assurance does not execute cargo itself; the release gate still requires rustfmt, clippy, tests, and a release build on a Rust 1.95.0 host.',
      evidence: ['package.json:check:rust', 'scripts/cargo-task.mjs', 'rust-toolchain.toml'],
      failureMode: 'Dashboard/static contracts are green but the shipped native binary fails to build or runtime paths fail under Cargo tests.',
      nextCheck: 'Run on a Rust host: rustup toolchain install 1.95.0 && npm run check:rust && cargo build --release.',
    }),
    claim({
      id: 'live-e2e-unverified-here',
      title: 'Live add-enable-test-upstream-call path needs one real target-machine proof',
      status: 'warn',
      why: 'Static checks show wiring and contracts, but user confidence also needs one real server and one real client call through /mcp.',
      evidence: ['scripts/load-test-local.mjs', 'src/dashboard/overview.rs:flow', 'tests/node/dashboard-contract.test.mjs'],
      failureMode: 'Everything is connected in source, but target OS process spawning, firewall, client config path, or upstream stdio runtime fails in practice.',
      nextCheck: 'Run mcpace up, add a harmless server, enable it, Test it, connect a client to /mcp, and execute one read-only upstream_call.',
    }),
  ];

  const failCount = claims.filter((item) => item.status === 'fail').length;
  const warnCount = claims.filter((item) => item.status === 'warn').length;
  const passCount = claims.filter((item) => item.status === 'pass').length;
  const overall = failCount > 0 ? 'fail' : warnCount > 0 ? 'needs-live-rust-proof' : 'pass';

  return {
    schema: 'mcpace.projectAssurance.v1',
    generatedAt: new Date().toISOString(),
    root: '.',
    rootName: path.basename(repoRoot),
    overall,
    summary: {
      pass: passCount,
      warn: warnCount,
      fail: failCount,
      worstStatus: worstStatus(claims),
    },
    reviewModel: [
      {
        question: 'What should a normal user see?',
        answer: 'One readiness verdict, endpoint, server launch command/URL, evidence state, and one safe next action.',
      },
      {
        question: 'What should an operator see?',
        answer: 'Per-server lane, blockers, safeguards, recommended policy, command runbook, and whether policy drift exists.',
      },
      {
        question: 'What should stay hidden by default?',
        answer: 'Secret values, raw JSON/logs, manual concurrency controls, and disabled tools represented as usable.',
      },
      {
        question: 'What proves the product is smarter?',
        answer: 'Backend evidence drives UI decisions; package names alone do not grant trust; runtimeControlPlane ties evidence, risk, isolation, resources, and parallelism into one reviewable model.',
      },
    ],
    correctVerificationFlow: [
      'Static source contract: npm run check:assurance and npm run check.',
      'Release contract: npm run build:release-artifacts and inspect zipVerification.status.',
      'Platform static contract: npm run check:platform and inspect reports/platform-proof.md.',
      'Rust host contract: npm run check:rust and cargo build --release on a machine with Rust 1.95.0.',
      'Live user contract: mcpace up, open dashboard, add one harmless server disabled, enable intentionally, run Test once.',
      'MCP client contract: point one client at /mcp, run tools/list, then one read-only upstream_call.',
      'Security negative contract: bad Origin/Host/content headers/session/request-id tests must fail closed.',
    ],
    claims,
  };
}

function renderMarkdown(report) {
  const lines = [];
  lines.push('# MCPace assurance review');
  lines.push('');
  lines.push('Generated by `npm run assurance`. This report answers: what is smart/useful, what is proven, what is still unproven, and how to review the product without fooling yourself.');
  lines.push('');
  lines.push(`- Overall: **${report.overall}**`);
  lines.push(`- Claims: ${report.summary.pass} pass, ${report.summary.warn} warn, ${report.summary.fail} fail`);
  lines.push('');
  lines.push('## How to look at the product');
  lines.push('');
  for (const item of report.reviewModel) {
    lines.push(`- **${item.question}** ${item.answer}`);
  }
  lines.push('');
  lines.push('## Correct verification flow');
  lines.push('');
  for (let index = 0; index < report.correctVerificationFlow.length; index += 1) {
    lines.push(`${index + 1}. ${report.correctVerificationFlow[index]}`);
  }
  lines.push('');
  lines.push('## Assurance claims');
  lines.push('');
  lines.push('| Status | Claim | Why it matters | Evidence | Next check |');
  lines.push('|---|---|---|---|---|');
  for (const item of report.claims) {
    const evidence = item.evidence.map((value) => `\`${value}\``).join('<br>');
    lines.push(`| ${item.status} | ${item.title.replace(/\|/g, '\\|')} | ${item.why.replace(/\|/g, '\\|')} | ${evidence || '—'} | ${item.nextCheck.replace(/\|/g, '\\|')} |`);
  }
  lines.push('');
  lines.push('## How to identify wrong vs right states');
  lines.push('');
  lines.push('- **Right:** empty install exposes no upstream tools; user adds one server intentionally; dashboard shows command, evidence, and one next action.');
  lines.push('- **Right:** ready means live `initialize + tools/list` evidence exists, not just a package name or config entry.');
  lines.push('- **Right:** sensitive/stateful/external servers stay serialized/session/project-isolated until stronger evidence exists.');
  lines.push('- **Wrong:** UI claims a server is usable while it is disabled, untested, missing launch metadata, or blocked by policy/source mismatch.');
  lines.push('- **Wrong:** a pasted command can chain shell operations, leak env/header values, or auto-enable tools without explicit Test.');
  lines.push('- **Wrong:** green Node tests are treated as production proof without Rust build and one live MCP client call.');
  lines.push('');
  return `${lines.join('\n')}\n`;
}

const report = buildAssurance();
if (write) {
  const reportsDir = path.join(repoRoot, 'reports');
  fs.mkdirSync(reportsDir, { recursive: true });
  writeFileAtomicSync(path.join(reportsDir, 'assurance.json'), JSON.stringify(report, null, 2) + '\n', { mode: 0o644 });
  writeFileAtomicSync(path.join(reportsDir, 'assurance.md'), renderMarkdown(report), { mode: 0o644 });
}

if (jsonOnly) {
  process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
} else if (check || args.has('--ci')) {
  process.stdout.write(`MCPace assurance: ${report.overall} (${report.summary.pass} pass, ${report.summary.warn} warn, ${report.summary.fail} fail)\n`);
} else if (!write) {
  process.stdout.write(renderMarkdown(report));
}

if ((check || args.has('--ci')) && report.summary.fail > 0) {
  process.stderr.write(`MCPace assurance failed: ${report.summary.fail} failed claim(s).\n`);
  process.exit(1);
}
