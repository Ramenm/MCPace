#!/usr/bin/env node
import { existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { profileFrom } from './lib/mcp-evidence-profile.mjs';

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');

function parseArgs(argv) {
  const args = {
    root: repoRoot,
    json: false,
    write: join(repoRoot, 'reports', 'adaptive-parallelism-latest.json'),
    markdown: join(repoRoot, 'reports', 'adaptive-parallelism-latest.md'),
    includeEdgeCases: true,
  };
  for (let i = 0; i < argv.length; i += 1) {
    const token = argv[i];
    if (token === '--root') args.root = resolve(argv[++i] || '.');
    else if (token === '--json') args.json = true;
    else if (token === '--write') args.write = resolve(argv[++i] || '');
    else if (token === '--markdown') args.markdown = resolve(argv[++i] || '');
    else if (token === '--no-write') {
      args.write = null;
      args.markdown = null;
    } else if (token === '--no-edge-cases') args.includeEdgeCases = false;
    else if (token === '--help' || token === '-h') args.help = true;
    else throw new Error(`unknown adaptive-parallelism-audit argument: ${token}`);
  }
  return args;
}

function printHelp() {
  console.log(`Usage: node scripts/adaptive-parallelism-audit.mjs [--json] [--no-write] [--write FILE] [--markdown FILE]\n\nAudits evidence-first MCP server profiling without a packaged upstream-server catalog.`);
}

function readJson(file, fallback = null) {
  try {
    if (!existsSync(file)) return fallback;
    return JSON.parse(readFileSync(file, 'utf8'));
  } catch (error) {
    return { __error: String(error?.message || error) };
  }
}

function loadConfiguredServers(root) {
  const config = readJson(join(root, 'mcpace.config.json'), {}) || {};
  const out = [];
  const addFromObject = (obj, source) => {
    if (!obj || typeof obj !== 'object') return;
    const servers = obj.mcpServers || obj.servers || {};
    if (!servers || typeof servers !== 'object') return;
    for (const [name, value] of Object.entries(servers)) {
      if (!value || typeof value !== 'object') continue;
      out.push({ ...value, serverId: name, name, source });
    }
  };
  addFromObject(readJson(join(root, 'mcp_settings.json'), {}), 'mcp_settings.json');
  const defaultDir = join(root, 'mcp_settings.d');
  if (existsSync(defaultDir)) {
    for (const entry of readdirSync(defaultDir).sort()) {
      const file = join(defaultDir, entry);
      if (entry.endsWith('.json') && statSync(file).isFile()) addFromObject(readJson(file, {}), `mcp_settings.d/${entry}`);
    }
  }
  for (const [name, value] of Object.entries(config.servers || {})) out.push({ ...value, serverId: name, name, source: 'mcpace.config.json' });
  return out;
}

function edgeCases() {
  return [
    { id: 'unknown-stdio-npx', raw: { serverId: 'unknown-stdio-npx', transport: 'stdio', launcher: 'npx', command: 'npx', args: ['-y', '@vendor/random-mcp'] }, expected: { parallelSafetyClass: 'P0_unknown_stdio', defaultPoolModel: 'process-pool', maxInFlightPerWorker: 1 }, rationale: 'Unknown stdio remains one in-flight until probes and policy evidence exist.' },
    { id: 'legacy-sse', raw: { serverId: 'legacy-sse', transport: 'sse-legacy', url: 'http://127.0.0.1:9000/sse' }, expected: { parallelSafetyClass: 'PX_legacy_compat', defaultPoolModel: 'legacy-disabled', maxWorkers: 0 }, rationale: 'Legacy SSE is not treated as modern Streamable HTTP scheduling.' },
    { id: 'remote-streamable-http', raw: { serverId: 'remote-streamable-http', transport: 'streamable-http', url: 'https://mcp.example.com/mcp' }, expected: { parallelSafetyClass: 'P2_session_safe', defaultPoolModel: 'remote-http-session-pool' }, rationale: 'Remote HTTP is session-bound until MCP-Session-Id/probe evidence proves otherwise.' },
    { id: 'stateless-remote-http', raw: { serverId: 'stateless-remote-http', transport: 'streamable-http', url: 'https://docs.example.com/mcp', policy: { stateless: true, concurrencyPolicy: 'multi-reader', stateBinding: 'none', parallelismLimit: 8 } }, expected: { parallelSafetyClass: 'P4_stateless_remote_candidate', defaultPoolModel: 'remote-http-shared-pool' }, rationale: 'Only explicit stateless evidence raises remote HTTP to broad fan-out.' },
    { id: 'credential-scoped-api', raw: { serverId: 'credential-scoped-api', transport: 'stdio', command: 'node', args: ['slack-mcp', '--token', '$SLACK_TOKEN'] }, expected: { parallelSafetyClass: 'P2_session_safe', defaultPoolModel: 'credential-session-pool' }, rationale: 'Credential/API surfaces need profile or tenant affinity.' },
    { id: 'project-filesystem-write', raw: { serverId: 'project-filesystem-write', transport: 'stdio', command: 'npx', args: ['@modelcontextprotocol/server-filesystem', '.'] }, expected: { parallelSafetyClass: 'P3_project_safe', defaultPoolModel: 'project-pool' }, rationale: 'Filesystem tools lock project/file domains.' },
    { id: 'repo-git-write', raw: { serverId: 'repo-git-write', transport: 'stdio', command: 'uvx', args: ['mcp-server-git', '--repository', '.'] }, expected: { parallelSafetyClass: 'P3_project_safe', defaultPoolModel: 'project-pool' }, rationale: 'Git tools lock repo/project domains.' },
    { id: 'browser-automation', raw: { serverId: 'browser-automation', transport: 'stdio', command: 'npx', args: ['@playwright/mcp'] }, expected: { parallelSafetyClass: 'PX_forbidden_browser_until_context_isolated', defaultPoolModel: 'session-pool' }, rationale: 'Browser automation cannot fan out without browser-context isolation.' },
    { id: 'shared-exclusive-desktop', raw: { serverId: 'shared-exclusive-desktop', transport: 'stdio', command: 'desktop-control-mcp', args: ['--profile', 'default'] }, expected: { parallelSafetyClass: 'PX_forbidden_browser_until_context_isolated', defaultPoolModel: 'session-pool' }, rationale: 'Desktop/profile control is shared-exclusive.' },
    { id: 'readonly-stdio-candidate', raw: { serverId: 'readonly-stdio-candidate', transport: 'stdio', command: 'uvx', args: ['mcp-server-time'], policy: { stateless: true, concurrencyPolicy: 'multi-reader', stateBinding: 'none' } }, expected: { parallelSafetyClass: 'P1_readonly_candidate', defaultPoolModel: 'process-pool' }, rationale: 'Small local utilities can become multi-reader after explicit read-only evidence.' },
    { id: 'stateful-memory', raw: { serverId: 'stateful-memory', transport: 'stdio', command: 'npx', args: ['@modelcontextprotocol/server-memory'] }, expected: { parallelSafetyClass: 'P2_session_safe', defaultPoolModel: 'singleton' }, rationale: 'Memory/context stores are session/profile stateful.' },
    { id: 'local-database', raw: { serverId: 'local-database', transport: 'stdio', command: 'uvx', args: ['mcp-server-sqlite', '--db-path', './data.db'] }, expected: { parallelSafetyClass: 'P3_project_safe', defaultPoolModel: 'project-pool' }, rationale: 'Local databases lock database/project domains.' },
    { id: 'oci-unknown', raw: { serverId: 'oci-unknown', transport: 'stdio', launcher: 'oci', command: 'docker', args: ['run', '--rm', 'vendor/mcp:latest'] }, expected: { parallelSafetyClass: 'P0_unknown_stdio', defaultPoolModel: 'process-pool', maxInFlightPerWorker: 1 }, rationale: 'Container images remain unknown until provenance and probes are reviewed.' },
  ];
}

function matchesExpected(actual, expected) {
  for (const [key, value] of Object.entries(expected || {})) {
    if (actual[key] !== value) return false;
  }
  return true;
}

function renderMarkdown(report) {
  const lines = [];
  lines.push('# Adaptive MCP parallelism audit');
  lines.push('');
  lines.push(`Generated: ${report.generatedAt}`);
  lines.push(`Status: **${report.status}**`);
  lines.push('');
  lines.push(`Configured profiles: ${report.summary.profileCount}; edge cases: ${report.summary.edgeCaseCount}; static catalogs present: ${report.summary.staticCatalogPresent ? 'yes' : 'no'}.`);
  lines.push('');
  lines.push('## Profiles');
  lines.push('');
  lines.push('| Server | Source | Transport | Launcher | Safety | Pool | Workers | Locks | Stateless |');
  lines.push('|---|---|---|---|---|---|---:|---|---:|');
  for (const profile of report.profiles) {
    lines.push(`| ${profile.serverId} | ${profile.source} | ${profile.transport} | ${profile.launcher} | ${profile.parallelSafetyClass} | ${profile.defaultPoolModel} | ${profile.maxWorkers} | ${(profile.lockDomains || []).join(', ') || 'none'} | ${profile.stateless ? 'yes' : 'no'} |`);
  }
  if (!report.profiles.length) lines.push('| none | empty by default | - | - | - | - | 0 | - | - |');
  lines.push('');
  lines.push('## Edge cases');
  lines.push('');
  for (const edge of report.edgeCases) lines.push(`- ${edge.ok ? 'PASS' : 'FAIL'} ${edge.id}: ${edge.rationale}`);
  lines.push('');
  lines.push('## Checks');
  lines.push('');
  for (const check of report.checks) lines.push(`- ${check.ok ? 'PASS' : 'FAIL'} ${check.id}: ${check.detail}`);
  if (report.blockers.length) {
    lines.push('');
    lines.push('## Blockers');
    lines.push('');
    for (const blocker of report.blockers) lines.push(`- ${blocker}`);
  }
  if (report.warnings.length) {
    lines.push('');
    lines.push('## Warnings');
    lines.push('');
    for (const warning of report.warnings) lines.push(`- ${warning}`);
  }
  lines.push('');
  return `${lines.join('\n')}\n`;
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) return printHelp();
  const root = args.root;
  const pkg = readJson(join(root, 'package.json'), {}) || {};
  const config = readJson(join(root, 'mcpace.config.json'), {}) || {};
  const configuredServers = loadConfiguredServers(root);
  const profiles = configuredServers.map((server) => profileFrom(server, server.source || 'settings'));
  const edgeCases = args.includeEdgeCases
    ? edgeCasesRaw().map((edge) => {
        const actual = profileFrom(edge.raw, 'edge-fixture');
        const ok = matchesExpected(actual, edge.expected);
        const locksOk = Array.isArray(actual.lockDomains) && actual.lockDomains.length > 0;
        return { ...edge, actual, ok, locksOk };
      })
    : [];
  const staticCatalogPresent = existsSync(join(root, 'presets')) || Boolean(config.mcpPresets);
  const checks = [
    { id: 'no-packaged-upstream-catalog', ok: !staticCatalogPresent, detail: 'Packaged upstream-server catalogs are absent; install/profile behavior is evidence-first.' },
    { id: 'auto-profile-config', ok: Boolean(config.autoProfile) && !config.mcpPresets, detail: 'mcpace.config.json documents automatic profiling and no longer exposes static server catalogs.' },
    { id: 'no-bundled-default-upstreams', ok: Object.keys(config.servers || {}).length === 0 && configuredServers.length === 0, detail: 'The project ships with no enabled upstream MCP servers by default.' },
    { id: 'edge-case-matrix', ok: !args.includeEdgeCases || edgeCases.length >= 13 && edgeCases.every((edge) => edge.ok && edge.locksOk), detail: 'Synthetic state/session/client edge cases classify to expected conservative plans.' },
    { id: 'unknown-is-conservative', ok: edgeCases.some((edge) => edge.id === 'unknown-stdio-npx' && edge.actual.parallelSafetyClass === 'P0_unknown_stdio' && edge.actual.maxInFlightPerWorker === 1), detail: 'Unknown stdio stays one in-flight and review-gated.' },
    { id: 'remote-session-default', ok: edgeCases.some((edge) => edge.id === 'remote-streamable-http' && edge.actual.parallelSafetyClass === 'P2_session_safe' && edge.actual.defaultPoolModel === 'remote-http-session-pool'), detail: 'Remote Streamable HTTP remains session-safe until stateless evidence exists.' },
    { id: 'live-probe-harness', ok: existsSync(join(root, 'scripts', 'live-random-mcp-probe.mjs')), detail: 'Random/live MCP package probe harness exists for package-derived evidence.' },
  ];
  const blockers = checks.filter((check) => !check.ok).map((check) => `${check.id}: ${check.detail}`);
  const warnings = [];
  if (!configuredServers.length) warnings.push('No configured upstream MCP servers in the source snapshot; only synthetic edge cases are profiled here.');
  for (const profile of profiles) {
    if (String(profile.parallelSafetyClass).startsWith('P0_')) warnings.push(`${profile.serverId}: unknown source needs probe evidence before concurrency is raised.`);
    if (String(profile.parallelSafetyClass).startsWith('PX_')) warnings.push(`${profile.serverId}: high-risk source needs explicit isolation before scheduling.`);
  }
  const statelessCount = [...profiles, ...edgeCases.map((edge) => edge.actual)].filter((profile) => profile.stateless).length;
  const statefulCount = [...profiles, ...edgeCases.map((edge) => edge.actual)].filter((profile) => !profile.stateless).length;
  const report = {
    schema: 'mcpace.adaptiveParallelismAudit.v2',
    generatedAt: new Date().toISOString(),
    root,
    project: { name: 'mcpace', version: pkg.version || '0.0.0' },
    summary: {
      profileCount: profiles.length,
      edgeCaseCount: edgeCases.length,
      stableCount: [...profiles, ...edgeCases.map((edge) => edge.actual)].filter((profile) => !String(profile.parallelSafetyClass).startsWith('P0_') && !String(profile.parallelSafetyClass).startsWith('PX_')).length,
      conservativeCount: [...profiles, ...edgeCases.map((edge) => edge.actual)].filter((profile) => String(profile.parallelSafetyClass).startsWith('P0_')).length,
      legacyCount: [...profiles, ...edgeCases.map((edge) => edge.actual)].filter((profile) => profile.defaultPoolModel === 'legacy-disabled').length,
      statefulCount,
      statelessCount,
      staticCatalogPresent,
    },
    profiles,
    edgeCases,
    checks,
    warnings,
    blockers,
    status: blockers.length ? 'blocked' : 'pass',
  };
  if (args.write) {
    mkdirSync(dirname(args.write), { recursive: true });
    writeFileSync(args.write, `${JSON.stringify(report, null, 2)}\n`);
  }
  if (args.markdown) {
    mkdirSync(dirname(args.markdown), { recursive: true });
    writeFileSync(args.markdown, renderMarkdown(report));
  }
  if (args.json) console.log(JSON.stringify(report, null, 2));
  else console.log(renderMarkdown(report));
  if (report.status !== 'pass') process.exitCode = 1;
}

function edgeCasesRaw() {
  return edgeCases();
}

try {
  main();
} catch (error) {
  console.error(error?.stack || String(error));
  process.exitCode = 1;
}
