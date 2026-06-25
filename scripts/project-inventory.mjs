#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { writeFileAtomicSync } from './lib/atomic-fs.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, '..');
const args = new Set(process.argv.slice(2));
const write = args.has('--write');
const jsonOnly = args.has('--json');

function posix(relativePath) {
  return relativePath.split(path.sep).join('/');
}

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

function walk(rootRelative, predicate = () => true) {
  const root = path.join(repoRoot, rootRelative);
  const out = [];
  const stack = [root];
  while (stack.length > 0) {
    const current = stack.pop();
    if (!fs.existsSync(current)) continue;
    for (const entry of fs.readdirSync(current, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
      const full = path.join(current, entry.name);
      const rel = posix(path.relative(repoRoot, full));
      if (entry.isDirectory()) {
        if (!['.git', 'node_modules', 'target', 'dist', '.cache'].includes(entry.name)) stack.push(full);
      } else if (entry.isFile() && predicate(rel, full)) {
        out.push(rel);
      }
    }
  }
  return out.sort();
}

function lineCount(text) {
  if (!text) return 0;
  return text.split(/\r?\n/).length;
}

function parseCommandCatalog() {
  const source = read('src/catalog.rs');
  const commands = [];
  for (const match of source.matchAll(/CommandSpec\s*\{([\s\S]*?)\n\s*\}/g)) {
    const block = match[1];
    const name = block.match(/name:\s*"([^"]+)"/)?.[1];
    if (!name) continue;
    const description = block.match(/description:\s*"([\s\S]*?)"/)?.[1]?.replace(/\s+/g, ' ') ?? '';
    const aliasBlock = block.match(/aliases:\s*&\[([\s\S]*?)\]/)?.[1] ?? '';
    const aliases = [...aliasBlock.matchAll(/"([^"]+)"/g)].map((entry) => entry[1]);
    const implemented = /implemented:\s*true/.test(block);
    commands.push({ name, aliases, implemented, description });
  }
  return commands;
}

function subcommandsFromParser(relativePath) {
  if (!fs.existsSync(path.join(repoRoot, relativePath))) return [];
  const source = read(relativePath);
  const tokens = new Set();
  for (const match of source.matchAll(/"([a-z][a-z0-9-]*)"/g)) {
    const value = match[1];
    if (!value.startsWith('-') && !['true', 'false', 'json', 'root', 'name'].includes(value)) {
      tokens.add(value);
    }
  }
  return [...tokens].sort();
}

function parseRustFunctions() {
  const files = walk('src', (rel) => rel.endsWith('.rs'));
  const functions = [];
  const duplicateMap = new Map();
  const longFiles = [];
  for (const file of files) {
    const text = read(file);
    const lines = lineCount(text);
    if (lines >= 700) longFiles.push({ file, lines });
    for (const match of text.matchAll(/^\s*(pub(?:\([^)]*\))?\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(/gm)) {
      const visibility = match[1]?.trim() || 'private';
      const item = { file, name: match[2], visibility };
      functions.push(item);
      const key = item.name;
      if (!duplicateMap.has(key)) duplicateMap.set(key, []);
      duplicateMap.get(key).push(`${file} (${visibility})`);
    }
  }
  const duplicates = [...duplicateMap.entries()]
    .filter(([, owners]) => owners.length > 1)
    .map(([name, owners]) => ({ name, owners }))
    .sort((a, b) => b.owners.length - a.owners.length || a.name.localeCompare(b.name));
  return { files, functions, duplicates, longFiles: longFiles.sort((a, b) => b.lines - a.lines) };
}

function parseNotImplemented() {
  const files = walk('src', (rel) => rel.endsWith('.rs'));
  const hits = [];
  const seen = new Set();
  const pattern = /not implemented|bootstrap-only|direct HTTPS upstream forwarding is not available|Cloud agent supports tools only/ig;
  for (const file of files) {
    const text = read(file);
    let match;
    while ((match = pattern.exec(text)) !== null) {
      const before = text.slice(0, match.index);
      const line = before.split(/\r?\n/).length;
      const lineText = text.split(/\r?\n/)[line - 1]?.trim() ?? '';
      const key = `${file}:${line}`;
      if (seen.has(key)) continue;
      seen.add(key);
      hits.push({ file, line, text: lineText.slice(0, 220) });
    }
  }
  return hits;
}


function sortedUnique(values) {
  return [...new Set(values)].sort((a, b) => a.localeCompare(b));
}

function parseMcpStdioTools() {
  const source = read('src/mcp_server/tool_surface.rs');
  return sortedUnique([...source.matchAll(/name:\s*"([a-z][a-z0-9_]*)"/g)].map((match) => match[1]));
}

function parseMcpHttpTools() {
  const source = read('src/dashboard/http_tools.rs');
  return sortedUnique([...source.matchAll(/http_tool(?:_with_schema)?\(\s*"([a-z][a-z0-9_]*)"/g)].map((match) => match[1]));
}

function parseHttpMcpMethods() {
  const source = read('src/dashboard/mcp_http.rs');
  const methods = new Set();
  for (const method of ['initialize', 'ping', 'tools/list', 'tools/call', 'prompts/list', 'prompts/get', 'resources/list', 'resources/templates/list', 'resources/read', 'notifications/initialized']) {
    if (source.includes(`"${method}"`)) methods.add(method);
  }
  return [...methods].sort((a, b) => a.localeCompare(b));
}

function toolDispatchCoverage(toolNames, relativePath) {
  const source = read(relativePath);
  return toolNames.map((name) => ({
    name,
    dispatched: source.includes(`"${name}"`),
  }));
}

function buildRuntimeFlowAudit() {
  const stdioTools = parseMcpStdioTools();
  const httpTools = parseMcpHttpTools();
  const onlyStdio = stdioTools.filter((name) => !httpTools.includes(name));
  const onlyHttp = httpTools.filter((name) => !stdioTools.includes(name));
  const httpDispatchCoverage = toolDispatchCoverage(httpTools, 'src/dashboard/tool_runtime.rs');
  const stdioDispatchCoverage = toolDispatchCoverage(stdioTools, 'src/mcp_server.rs');

  return {
    schema: 'mcpace.runtimeFlowAudit.v1',
    mcpHttpMethods: parseHttpMcpMethods(),
    surfaces: {
      stdioToolCount: stdioTools.length,
      httpToolCount: httpTools.length,
      commonToolCount: stdioTools.filter((name) => httpTools.includes(name)).length,
      stdioTools,
      httpTools,
      onlyStdio,
      onlyHttp,
      note: 'Differences are not automatically bugs: stdio exposes explicit lease primitives, while Streamable HTTP routes upstream calls through request/session context and exposes diagnostics instead.',
    },
    dispatchCoverage: {
      stdio: stdioDispatchCoverage,
      http: httpDispatchCoverage,
      missingStdioDispatch: stdioDispatchCoverage.filter((item) => !item.dispatched).map((item) => item.name),
      missingHttpDispatch: httpDispatchCoverage.filter((item) => !item.dispatched).map((item) => item.name),
    },
    endToEndFlows: [
      {
        name: 'Quickstart / up',
        trigger: 'mcpace up / mcpace setup',
        path: ['src/app.rs', 'src/setup.rs', 'src/init.rs', 'src/serve.rs', 'src/client/actions.rs', 'src/verify/*'],
        usefulWork: 'Create or repair the MCPace home, import existing MCP settings, start the local endpoint, wire supported clients, and run readiness checks without adding default upstream servers.',
        safetyBoundary: 'No default filesystem/memory/server is installed unless the user imports or installs one.',
      },
      {
        name: 'Client wiring',
        trigger: 'mcpace client install/export/plan/restore',
        path: ['src/client.rs', 'src/client/args.rs', 'src/client/actions.rs', 'src/client/actions/config_update.rs', 'src/client/plan.rs'],
        usefulWork: 'Detect supported clients, generate one MCPace endpoint entry, preserve existing user MCP servers, and keep backup/restore paths.',
        safetyBoundary: 'Client mutation stays inside client actions; setup only orchestrates.',
      },
      {
        name: 'Server inventory and policy',
        trigger: 'mcpace server list/import/install/auto/set-policy/instances/test',
        path: ['src/server.rs', 'src/server/*', 'src/mcp_sources/*', 'src/source_type.rs', 'src/adapter/discovery.rs'],
        usefulWork: 'Merge MCP settings, normalize source shapes, infer runtime policy, write explicit server fragments, and plan instances/leases.',
        safetyBoundary: 'Unknown packages stay review/plan-only unless trust gates allow install and probe.',
      },
      {
        name: 'Streamable HTTP MCP endpoint',
        trigger: 'POST /mcp initialize -> notifications/initialized -> tools/list -> tools/call',
        path: ['src/dashboard/mcp_http.rs', 'src/dashboard/http_boundary.rs', 'src/dashboard/http_session.rs', 'src/dashboard/http_tools.rs', 'src/dashboard/tool_runtime.rs'],
        usefulWork: 'Enforce local HTTP boundary checks, negotiate MCP protocol/session, list management and broker tools, and execute tool calls.',
        safetyBoundary: 'Origin/Accept/Content-Type/session checks are centralized before tool execution.',
      },
      {
        name: 'Upstream broker call',
        trigger: 'upstream_search/upstream_tools/upstream_call/upstream_batch or projected u_* tool',
        path: ['src/dashboard/tool_runtime.rs', 'src/mcp_server.rs', 'src/adapter.rs', 'src/upstream.rs', 'src/upstream/lease_runtime.rs', 'src/upstream/stdio_runtime.rs', 'src/upstream/http_runtime.rs'],
        usefulWork: 'Discover live upstream tools, verify requested tools against tools/list, acquire routing context, then call stdio or local Streamable HTTP upstreams through scheduler-aware pooling.',
        safetyBoundary: 'No hidden dynamic upstream tool call is allowed unless explicitly trusted through the override argument.',
      },
      {
        name: 'Observability / operator UI',
        trigger: 'mcpace dashboard / /api/overview / runtime_diagnostics',
        path: ['src/dashboard.rs', 'src/dashboard/*', 'src/hub/status.rs', 'src/upstream/diagnostics.rs'],
        usefulWork: 'Show endpoint health, configured servers, warnings, leases, logs, and runtime/upstream availability without inventing unavailable telemetry.',
        safetyBoundary: 'Process-level CPU/RAM/latency histograms remain explicit telemetry gaps until runtime collection exists.',
      },
    ],
  };
}

function stripQuery(value) {
  return value.split('?')[0];
}

function parseDashboardBackendRoutes() {
  const source = read('src/dashboard.rs');
  const routes = [];
  for (const match of source.matchAll(/\("(GET|POST|DELETE)",\s*"([^"]+)"\)/g)) {
    routes.push({ method: match[1], path: match[2], dynamic: false });
  }
  // These are configured by mcpace.config/runtimepaths before the static match arms.
  routes.push({ method: 'GET', path: '/healthz', dynamic: true });
  routes.push({ method: 'GET', path: '/mcp', dynamic: true });
  routes.push({ method: 'POST', path: '/mcp', dynamic: true });
  routes.push({ method: 'DELETE', path: '/mcp', dynamic: true });
  return routes.sort((a, b) => `${a.method} ${a.path}`.localeCompare(`${b.method} ${b.path}`));
}

function parseDashboardFrontendEndpoints() {
  const html = read('src/dashboard/index.html');
  const endpoints = new Set();
  for (const match of html.matchAll(/(?:timedFetchJson|fetchJson|runAction)\(\s*"([^"]+)"/g)) {
    const endpoint = match[1];
    if (endpoint.startsWith('/')) endpoints.add(endpoint);
  }
  return [...endpoints].sort((a, b) => a.localeCompare(b));
}

function parseDashboardFrontendServerActions() {
  const html = read('src/dashboard/index.html');
  const actions = new Set();
  for (const match of html.matchAll(/(?:runServerAction|postServerAction)\(\s*"(server-[a-z-]+)"/g)) {
    actions.add(match[1]);
  }
  for (const match of html.matchAll(/"(server-(?:enable|disable|policy|autotune|test|install-command))"/g)) {
    actions.add(match[1]);
  }
  return [...actions].sort((a, b) => a.localeCompare(b));
}

function buildDashboardContractAudit() {
  const html = read('src/dashboard/index.html');
  const routes = parseDashboardBackendRoutes();
  const routeSet = new Set(routes.map((route) => `${route.method} ${route.path}`));
  const frontendEndpoints = parseDashboardFrontendEndpoints();
  const frontendActions = parseDashboardFrontendServerActions();
  const missingFrontendRoutes = [];

  for (const endpoint of frontendEndpoints) {
    if (endpoint === '/api/actions/${endpoint}') continue;
    const method = endpoint.includes('/api/actions/') ? 'POST' : 'GET';
    const key = `${method} ${stripQuery(endpoint)}`;
    if (!routeSet.has(key)) missingFrontendRoutes.push(key);
  }

  for (const action of frontendActions) {
    const key = `POST /api/actions/${action}`;
    if (!routeSet.has(key)) missingFrontendRoutes.push(key);
  }

  const markupIds = new Set([...html.matchAll(/\bid="([^"]+)"/g)].map((match) => match[1]));
  const registeredIds = [...html.matchAll(/\$\("([^"]+)"\)/g)].map((match) => match[1]);
  const missingElementIds = sortedUnique(registeredIds.filter((id) => !markupIds.has(id)));
  const backend = read('src/dashboard.rs');
  const overview = read('src/dashboard/overview.rs');
  const payloadKeys = ['server', 'name', 'mode', 'maxWorkers', 'maxInFlightPerWorker', 'timeoutMs', 'changes', 'commandLine', 'force', 'disabled', 'dryRun'];
  const scriptMatch = html.match(/<script>([\s\S]*?)<\/script>/);
  let scriptParses = false;
  if (scriptMatch) {
    try {
      // Static parse only; do not execute dashboard code inside inventory.
      new Function(scriptMatch[1]);
      scriptParses = true;
    } catch (_) {
      scriptParses = false;
    }
  }

  return {
    schema: 'mcpace.dashboardContractAudit.v1',
    backendRoutes: routes,
    frontendEndpoints,
    frontendServerActions: frontendActions,
    missingFrontendRoutes: sortedUnique(missingFrontendRoutes),
    registeredElementCount: registeredIds.length,
    missingElementIds,
    payloadKeys,
    payloadParserCoverage: payloadKeys.map((key) => ({
      key,
      frontendMentions: html.includes(key),
      backendMentions: backend.includes(key),
    })),
    operatorPlan: {
      backendSchema: overview.includes('mcpace.operatorPlan.v1'),
      overviewField: overview.includes('"operatorPlan"'),
      uiPanel: html.includes('id="operator-plan-panel"'),
      uiRenderer: html.includes('renderOperatorPlan(overview.operatorPlan'),
      serverRunbook: html.includes('renderServerRunbook'),
      commandPreflight: html.includes('installCommandIntent') && backend.includes('command_line_uses_shell_composition'),
      embeddedScriptParses: scriptParses,
    },
    userReadiness: {
      backendSchema: overview.includes('mcpace.userReadiness.v1'),
      overviewField: overview.includes('"userReadiness"'),
      uiPanel: html.includes('id="user-readiness-title"'),
      uiRenderer: html.includes('renderUserReadiness(overview.userReadiness'),
      visibilityRules: overview.includes('shouldSee') && overview.includes('shouldHide'),
    },
    runtimeControl: {
      backendSchema: overview.includes('mcpace.runtimeControlPlane.v1'),
      overviewField: overview.includes('"runtimeControlPlane"'),
      resourceSchema: overview.includes('mcpace.serverResourceMonitoring.v1'),
      resourceEndpointField: overview.includes('"serverResourceMonitoring"'),
      uiRenderer: html.includes('renderRuntimeControl'),
      uiLookup: html.includes('runtimeControlForServer'),
      resourceUi: html.includes('Server resources'),
      processSnapshot: read('src/resources.rs').includes('process_resource_snapshot_json'),
      sessionSnapshots: read('src/upstream/session_pool.rs').includes('session_snapshots'),
    },
    note: 'Dashboard UI calls /api/overview, /api/logs, /api/resources, /api/actions/*, shows per-server launch metadata, receives backend operatorPlan lanes/runbooks, userReadiness, runtimeControlPlane decisions, and live serverResourceMonitoring snapshots when upstream sessions are running.',
  };
}


function parseAssuranceReport() {
  const reportPath = path.join(repoRoot, 'reports/assurance.json');
  if (!fs.existsSync(reportPath)) {
    return {
      present: false,
      overall: 'not-generated',
      pass: 0,
      warn: 0,
      fail: 0,
      note: 'Run npm run assurance to generate the project assurance report.',
    };
  }
  try {
    const report = JSON.parse(fs.readFileSync(reportPath, 'utf8'));
    return {
      present: true,
      overall: report.overall || 'unknown',
      pass: Number(report.summary?.pass || 0),
      warn: Number(report.summary?.warn || 0),
      fail: Number(report.summary?.fail || 0),
      note: 'Assurance report checks user-visible truth, safety boundaries, release reproducibility, and unverified Rust/live gates.',
    };
  } catch (error) {
    return {
      present: false,
      overall: 'invalid',
      pass: 0,
      warn: 0,
      fail: 1,
      note: `Could not parse reports/assurance.json: ${error.message}`,
    };
  }
}

function parsePlatformProofReport() {
  const reportPath = path.join(repoRoot, 'reports/platform-proof.json');
  if (!fs.existsSync(reportPath)) {
    return {
      present: false,
      overall: 'not-generated',
      platforms: [],
      workflowPlatforms: [],
      publishedTargetCount: 0,
      smokeCommandCount: 0,
      note: 'Run npm run platform to generate the platform proof report.',
    };
  }
  try {
    const report = JSON.parse(fs.readFileSync(reportPath, 'utf8'));
    return {
      present: true,
      overall: report.overall || 'unknown',
      platforms: report.platforms?.published || [],
      workflowPlatforms: report.platforms?.workflow || [],
      publishedTargetCount: Number(report.summary?.publishedTargetCount || 0),
      smokeCommandCount: Number(report.summary?.smokeCommandCount || 0),
      note: report.uiDecision?.decision || 'Platform proof covers native OS targets and binary smoke commands.',
    };
  } catch (error) {
    return {
      present: false,
      overall: 'invalid',
      platforms: [],
      workflowPlatforms: [],
      publishedTargetCount: 0,
      smokeCommandCount: 0,
      note: `Could not parse reports/platform-proof.json: ${error.message}`,
    };
  }
}

function buildInventory() {
  const rust = parseRustFunctions();
  const commands = parseCommandCatalog();
  const aliasOwners = new Map();
  const aliasDuplicates = [];
  for (const command of commands) {
    for (const alias of command.aliases) {
      const prev = aliasOwners.get(alias);
      if (prev && prev !== command.name) aliasDuplicates.push({ alias, owners: [prev, command.name] });
      aliasOwners.set(alias, command.name);
    }
  }
  const npmBin = 'packages/npm/cli/bin/mcpace.js';
  const npmBinPath = path.join(repoRoot, npmBin);
  return {
    schema: 'mcpace.projectInventory.v1',
    generatedAt: new Date().toISOString(),
    root: '.',
    rootName: path.basename(repoRoot),
    commands,
    groupedSubcommands: {
      server: subcommandsFromParser('src/server/args.rs'),
      client: subcommandsFromParser('src/client/args.rs'),
      hub: subcommandsFromParser('src/hub/args.rs'),
      verify: subcommandsFromParser('src/verify/args.rs'),
      lab: subcommandsFromParser('src/lab/args.rs'),
    },
    counts: {
      rustFiles: rust.files.length,
      rustFunctions: rust.functions.length,
      publicCommands: commands.length,
      implementedPublicCommands: commands.filter((command) => command.implemented).length,
      duplicateFunctionNames: rust.duplicates.length,
      longRustFiles: rust.longFiles.length,
    },
    npmLauncher: {
      binPath: npmBin,
      exists: fs.existsSync(npmBinPath),
      executable: fs.existsSync(npmBinPath) ? (fs.statSync(npmBinPath).mode & 0o111) !== 0 : false,
    },
    architectureSlices: [
      { slice: 'CLI dispatch', owners: ['src/app.rs', 'src/catalog.rs'] },
      { slice: 'Setup/up orchestration', owners: ['src/setup.rs', 'src/init.rs', 'src/serve.rs', 'src/service.rs'] },
      { slice: 'Client wiring', owners: ['src/client.rs', 'src/client/*', 'src/client_catalog.rs'] },
      { slice: 'Server inventory/install/policy', owners: ['src/server.rs', 'src/server/*', 'src/mcp_sources/*'] },
      { slice: 'Runtime scheduling/leases', owners: ['src/hub/*', 'src/client/plan.rs', 'src/upstream/lease_runtime.rs'] },
      { slice: 'HTTP MCP/dashboard boundary', owners: ['src/dashboard.rs', 'src/dashboard/*'] },
      { slice: 'Upstream execution', owners: ['src/upstream.rs', 'src/upstream/*'] },
      { slice: 'Verification/evidence', owners: ['src/verify/*', 'src/lab/*', 'eval/*'] },
      { slice: 'Release/npm packaging', owners: ['packages/npm/cli/*', 'scripts/build-release-artifacts.mjs', 'release-manifest.json'] },
      { slice: 'Platform proof', owners: ['release-targets.json', '.github/workflows/platform-proof.yml', 'scripts/platform-proof.mjs', 'scripts/platform-binary-smoke.mjs'] },
    ],
    longRustFiles: rust.longFiles.slice(0, 20),
    duplicateFunctionNames: rust.duplicates.slice(0, 30),
    unsupportedMarkers: parseNotImplemented(),
    commandAliasDuplicates: aliasDuplicates,
    runtimeFlow: buildRuntimeFlowAudit(),
    dashboardContract: buildDashboardContractAudit(),
    platformProof: parsePlatformProofReport(),
    assurance: parseAssuranceReport(),
  };
}

function renderMarkdown(inventory) {
  const lines = [];
  lines.push('# MCPace internal inventory');
  lines.push('');
  lines.push('Generated by `npm run inventory`. This report is static and dependency-free; it is meant to show ownership, duplicate pressure, unfinished surfaces, and command coverage before larger refactors.');
  lines.push('');
  lines.push('## Counts');
  lines.push('');
  lines.push(`- Rust files: ${inventory.counts.rustFiles}`);
  lines.push(`- Rust functions parsed: ${inventory.counts.rustFunctions}`);
  lines.push(`- Public command groups: ${inventory.counts.publicCommands}`);
  lines.push(`- Implemented public command groups: ${inventory.counts.implementedPublicCommands}`);
  lines.push(`- Duplicate function names to review: ${inventory.counts.duplicateFunctionNames}`);
  lines.push(`- Rust files at or above 700 lines: ${inventory.counts.longRustFiles}`);
  lines.push(`- npm launcher present/executable: ${inventory.npmLauncher.exists ? 'yes' : 'no'} / ${inventory.npmLauncher.executable ? 'yes' : 'no'}`);
  lines.push(`- MCP HTTP methods detected: ${inventory.runtimeFlow.mcpHttpMethods.length}`);
  lines.push(`- MCP tool surfaces: stdio ${inventory.runtimeFlow.surfaces.stdioToolCount}, HTTP ${inventory.runtimeFlow.surfaces.httpToolCount}, common ${inventory.runtimeFlow.surfaces.commonToolCount}`);
  lines.push(`- Dashboard backend/frontend contract: ${inventory.dashboardContract.missingFrontendRoutes.length === 0 && inventory.dashboardContract.missingElementIds.length === 0 ? 'connected' : 'needs review'}`);
  lines.push(`- Dashboard operator plan: ${inventory.dashboardContract.operatorPlan.backendSchema && inventory.dashboardContract.operatorPlan.uiRenderer && inventory.dashboardContract.operatorPlan.serverRunbook ? 'connected' : 'needs review'}`);
  lines.push(`- Dashboard user-readiness view: ${inventory.dashboardContract.userReadiness.backendSchema && inventory.dashboardContract.userReadiness.uiRenderer && inventory.dashboardContract.userReadiness.visibilityRules ? 'connected' : 'needs review'}`);
  lines.push(`- Dashboard runtime-control plane: ${inventory.dashboardContract.runtimeControl.backendSchema && inventory.dashboardContract.runtimeControl.resourceSchema && inventory.dashboardContract.runtimeControl.uiRenderer ? 'connected' : 'needs review'}`);
  lines.push(`- Platform proof: ${inventory.platformProof.overall} (${inventory.platformProof.platforms.join(', ') || 'no platforms'}, smoke commands ${inventory.platformProof.smokeCommandCount})`);
  lines.push(`- Project assurance: ${inventory.assurance.overall} (${inventory.assurance.pass} pass, ${inventory.assurance.warn} warn, ${inventory.assurance.fail} fail)`);
  lines.push('');
  lines.push('## Architecture slices');
  lines.push('');
  lines.push('| Slice | Owners |');
  lines.push('|---|---|');
  for (const slice of inventory.architectureSlices) {
    lines.push(`| ${slice.slice} | ${slice.owners.map((owner) => `\`${owner}\``).join(', ')} |`);
  }
  lines.push('');
  lines.push('## Public command groups');
  lines.push('');
  lines.push('| Command | Aliases | Implemented | Job |');
  lines.push('|---|---|---:|---|');
  for (const command of inventory.commands) {
    lines.push(`| \`${command.name}\` | ${command.aliases.length ? command.aliases.map((alias) => `\`${alias}\``).join(', ') : '—'} | ${command.implemented ? 'yes' : 'no'} | ${command.description || '—'} |`);
  }
  lines.push('');
  lines.push('## Grouped subcommands');
  lines.push('');
  for (const [group, commands] of Object.entries(inventory.groupedSubcommands)) {
    lines.push(`- \`${group}\`: ${commands.map((command) => `\`${command}\``).join(', ') || '—'}`);
  }
  lines.push('');
  lines.push('## Largest Rust files');
  lines.push('');
  lines.push('| Lines | File | Refactor direction |');
  lines.push('|---:|---|---|');
  for (const item of inventory.longRustFiles) {
    const direction = item.file.includes('/tests') || item.file.endsWith('/tests.rs')
      ? 'Test fixture/helper split only when edits become painful.'
      : 'Keep public API stable; split by parser/model/render/runtime responsibilities.';
    lines.push(`| ${item.lines} | \`${item.file}\` | ${direction} |`);
  }
  lines.push('');
  lines.push('## Duplicate function-name pressure');
  lines.push('');
  lines.push('Duplicate names are not automatically bugs. They are a review queue for helpers that may need centralization if logic, not just naming, overlaps.');
  lines.push('');
  lines.push('| Function | Owners |');
  lines.push('|---|---|');
  for (const item of inventory.duplicateFunctionNames.slice(0, 20)) {
    lines.push(`| \`${item.name}\` | ${item.owners.map((owner) => `\`${owner}\``).join('<br>')} |`);
  }
  lines.push('');
  lines.push('## End-to-end runtime flow map');
  lines.push('');
  lines.push('| Flow | Trigger | Main code path | Useful work | Safety boundary |');
  lines.push('|---|---|---|---|---|');
  for (const flow of inventory.runtimeFlow.endToEndFlows) {
    lines.push(`| ${flow.name} | \`${flow.trigger}\` | ${flow.path.map((owner) => `\`${owner}\``).join(' → ')} | ${flow.usefulWork.replace(/\|/g, '\\|')} | ${flow.safetyBoundary.replace(/\|/g, '\\|')} |`);
  }
  lines.push('');
  lines.push('## MCP surface connectivity');
  lines.push('');
  lines.push(`- HTTP MCP methods: ${inventory.runtimeFlow.mcpHttpMethods.map((method) => `\`${method}\``).join(', ') || '—'}`);
  lines.push(`- Stdio-only management tools: ${inventory.runtimeFlow.surfaces.onlyStdio.map((tool) => `\`${tool}\``).join(', ') || '—'}`);
  lines.push(`- HTTP-only management tools: ${inventory.runtimeFlow.surfaces.onlyHttp.map((tool) => `\`${tool}\``).join(', ') || '—'}`);
  lines.push(`- Surface note: ${inventory.runtimeFlow.surfaces.note}`);
  lines.push(`- Missing stdio dispatch entries: ${inventory.runtimeFlow.dispatchCoverage.missingStdioDispatch.map((tool) => `\`${tool}\``).join(', ') || 'none'}`);
  lines.push(`- Missing HTTP dispatch entries: ${inventory.runtimeFlow.dispatchCoverage.missingHttpDispatch.map((tool) => `\`${tool}\``).join(', ') || 'none'}`);
  lines.push('');
  lines.push('## Dashboard backend/frontend contract');
  lines.push('');
  lines.push(`- Backend routes handled: ${inventory.dashboardContract.backendRoutes.map((route) => `\`${route.method} ${route.path}\``).join(', ') || '—'}`);
  lines.push(`- Frontend endpoints used: ${inventory.dashboardContract.frontendEndpoints.map((endpoint) => `\`${endpoint}\``).join(', ') || '—'}`);
  lines.push(`- Frontend server actions used: ${inventory.dashboardContract.frontendServerActions.map((action) => `\`${action}\``).join(', ') || '—'}`);
  lines.push(`- Missing frontend routes: ${inventory.dashboardContract.missingFrontendRoutes.map((route) => `\`${route}\``).join(', ') || 'none'}`);
  lines.push(`- Missing registered element ids: ${inventory.dashboardContract.missingElementIds.map((id) => `\`${id}\``).join(', ') || 'none'}`);
  lines.push(`- Payload keys checked: ${inventory.dashboardContract.payloadKeys.map((key) => `\`${key}\``).join(', ')}`);
  lines.push(`- Operator plan backend schema: ${inventory.dashboardContract.operatorPlan.backendSchema ? 'yes' : 'no'}`);
  lines.push(`- Operator plan UI panel/renderer/runbook: ${inventory.dashboardContract.operatorPlan.uiPanel ? 'yes' : 'no'} / ${inventory.dashboardContract.operatorPlan.uiRenderer ? 'yes' : 'no'} / ${inventory.dashboardContract.operatorPlan.serverRunbook ? 'yes' : 'no'}`);
  lines.push(`- User-readiness backend schema: ${inventory.dashboardContract.userReadiness.backendSchema ? 'yes' : 'no'}`);
  lines.push(`- User-readiness UI panel/renderer/visibility rules: ${inventory.dashboardContract.userReadiness.uiPanel ? 'yes' : 'no'} / ${inventory.dashboardContract.userReadiness.uiRenderer ? 'yes' : 'no'} / ${inventory.dashboardContract.userReadiness.visibilityRules ? 'yes' : 'no'}`);
  lines.push(`- Runtime-control backend/resource schemas: ${inventory.dashboardContract.runtimeControl.backendSchema ? 'yes' : 'no'} / ${inventory.dashboardContract.runtimeControl.resourceSchema ? 'yes' : 'no'}`);
  lines.push(`- Runtime-control UI renderer/resource panel: ${inventory.dashboardContract.runtimeControl.uiRenderer ? 'yes' : 'no'} / ${inventory.dashboardContract.runtimeControl.resourceUi ? 'yes' : 'no'}`);
  lines.push(`- Runtime resource snapshots wired: process ${inventory.dashboardContract.runtimeControl.processSnapshot ? 'yes' : 'no'}, sessions ${inventory.dashboardContract.runtimeControl.sessionSnapshots ? 'yes' : 'no'}`);
  lines.push(`- Add-server command preflight: ${inventory.dashboardContract.operatorPlan.commandPreflight ? 'yes' : 'no'}`);
  lines.push(`- Embedded dashboard script parses: ${inventory.dashboardContract.operatorPlan.embeddedScriptParses ? 'yes' : 'no'}`);
  lines.push(`- Contract note: ${inventory.dashboardContract.note}`);
  lines.push('');
  lines.push('## Platform proof');
  lines.push('');
  lines.push(`- Present: ${inventory.platformProof.present ? 'yes' : 'no'}`);
  lines.push(`- Overall: ${inventory.platformProof.overall}`);
  lines.push(`- Published platforms: ${inventory.platformProof.platforms.map((platform) => `\`${platform}\``).join(', ') || '—'}`);
  lines.push(`- Workflow platforms: ${inventory.platformProof.workflowPlatforms.map((platform) => `\`${platform}\``).join(', ') || '—'}`);
  lines.push(`- Published targets: ${inventory.platformProof.publishedTargetCount}`);
  lines.push(`- Native smoke commands: ${inventory.platformProof.smokeCommandCount}`);
  lines.push(`- Console decision: ${inventory.platformProof.note}`);
  lines.push('');
  lines.push('## Assurance review');
  lines.push('');
  lines.push(`- Present: ${inventory.assurance.present ? 'yes' : 'no'}`);
  lines.push(`- Overall: ${inventory.assurance.overall}`);
  lines.push(`- Claims: ${inventory.assurance.pass} pass, ${inventory.assurance.warn} warn, ${inventory.assurance.fail} fail`);
  lines.push(`- Note: ${inventory.assurance.note}`);
  lines.push('');
  lines.push('## Known unfinished or intentionally bounded surfaces');
  lines.push('');
  lines.push('| File | Line | Marker |');
  lines.push('|---|---:|---|');
  for (const item of inventory.unsupportedMarkers.slice(0, 40)) {
    lines.push(`| \`${item.file}\` | ${item.line} | ${item.text.replace(/\|/g, '\\|')} |`);
  }
  lines.push('');
  lines.push('## Refactor rules');
  lines.push('');
  lines.push('1. Do not add new default upstream servers in setup; import or install only when the user supplied a source.');
  lines.push('2. Keep client config mutation in `src/client/actions/*`; setup may orchestrate it but should not rewrite client config directly.');
  lines.push('3. Keep source-type normalization in `src/source_type.rs`; do not re-add local normalizers.');
  lines.push('4. Keep MCP HTTP boundary checks centralized in `src/dashboard/http_boundary.rs` and response headers in `src/dashboard/response.rs`.');
  lines.push('5. Split large runtime files only along existing seams: args/model/render/runtime/tests. Avoid parallel duplicate implementations.');
  lines.push('');
  return `${lines.join('\n')}\n`;
}

const inventory = buildInventory();
if (write) {
  const reportsDir = path.join(repoRoot, 'reports');
  fs.mkdirSync(reportsDir, { recursive: true });
  writeFileAtomicSync(path.join(reportsDir, 'internal-inventory.json'), JSON.stringify(inventory, null, 2) + '\n', { mode: 0o644 });
  writeFileAtomicSync(path.join(reportsDir, 'internal-inventory.md'), renderMarkdown(inventory), { mode: 0o644 });
}

if (jsonOnly) {
  process.stdout.write(JSON.stringify(inventory, null, 2) + '\n');
} else if (!write) {
  process.stdout.write(renderMarkdown(inventory));
} else {
  process.stdout.write('Wrote reports/internal-inventory.json and reports/internal-inventory.md\n');
}
