#!/usr/bin/env node
import { readFileSync, writeFileSync, existsSync, readdirSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';

function argValue(flag, fallback = undefined) {
  const index = process.argv.indexOf(flag);
  return index >= 0 ? process.argv[index + 1] : fallback;
}
function has(flag) {
  return process.argv.includes(flag);
}
function readJson(path, fallback) {
  try {
    return JSON.parse(readFileSync(path, 'utf8'));
  } catch {
    return fallback;
  }
}
function writeMarkdown(path, report) {
  const lines = [];
  lines.push('# Adaptive parallelism audit');
  lines.push('');
  lines.push(`Status: **${report.status}**`);
  lines.push(`Generated: ${report.generatedAt}`);
  lines.push('');
  lines.push('## Summary');
  lines.push('');
  lines.push(`- Profiles inspected: ${report.summary.profileCount}`);
  lines.push(`- Edge-case fixtures: ${report.summary.edgeCaseCount}`);
  lines.push(`- Stable/default profiles: ${report.summary.stableCount}`);
  lines.push(`- Conservative/unknown profiles: ${report.summary.conservativeCount}`);
  lines.push(`- Legacy compatibility profiles: ${report.summary.legacyCount}`);
  lines.push(`- Blockers: ${report.blockers.length}`);
  lines.push(`- Warnings: ${report.warnings.length}`);
  lines.push('');
  lines.push('## Runtime/config profiles');
  lines.push('');
  lines.push('| Server | Source | Transport | Launcher | Safety class | Pool | Workers | In-flight/worker | Lock domains |');
  lines.push('|---|---|---|---|---|---|---:|---:|---|');
  for (const profile of report.profiles) {
    lines.push(`| ${profile.serverId} | ${profile.source} | ${profile.transport} | ${profile.launcher} | ${profile.parallelSafetyClass} | ${profile.defaultPoolModel} | ${profile.maxWorkers} | ${profile.maxInFlightPerWorker} | ${profile.lockDomains.join(', ') || 'none'} |`);
  }
  lines.push('');
  lines.push('## Edge-case matrix');
  lines.push('');
  lines.push('| Case | Expected | Actual | Status | Rationale |');
  lines.push('|---|---|---|---|---|');
  for (const edgeCase of report.edgeCases) {
    const expected = `${edgeCase.expected.parallelSafetyClass}/${edgeCase.expected.defaultPoolModel}/${edgeCase.expected.maxInFlightPerWorker}`;
    const actual = `${edgeCase.actual.parallelSafetyClass}/${edgeCase.actual.defaultPoolModel}/${edgeCase.actual.maxInFlightPerWorker}`;
    lines.push(`| ${edgeCase.id} | ${expected} | ${actual} | ${edgeCase.ok ? 'PASS' : 'FAIL'} | ${edgeCase.rationale} |`);
  }
  lines.push('');
  lines.push('## Checks');
  lines.push('');
  for (const check of report.checks) {
    lines.push(`- ${check.ok ? 'PASS' : 'FAIL'} ${check.id}: ${check.detail}`);
  }
  if (report.blockers.length) {
    lines.push('');
    lines.push('## Blockers');
    for (const blocker of report.blockers) lines.push(`- ${blocker}`);
  }
  if (report.warnings.length) {
    lines.push('');
    lines.push('## Warnings');
    for (const warning of report.warnings) lines.push(`- ${warning}`);
  }
  writeFileSync(path, `${lines.join('\n')}\n`);
}
function normalizeTransport(kind, url) {
  const raw = String(kind || '').trim().toLowerCase();
  if (['streamable-http', 'streamablehttp', 'http-stream', 'remote-http', 'remote', 'http'].includes(raw)) return 'streamable-http';
  if (['sse', 'remote-sse', 'http+sse', 'http-sse', 'legacy-sse'].includes(raw)) return 'sse-legacy';
  if (['stdio', 'local', 'local-stdio', 'local-command', 'command'].includes(raw)) return 'stdio';
  if (!raw && url) return 'streamable-http';
  return raw || 'stdio';
}
function launcherFor(command, url, pkg = '') {
  const c = String(command || '').toLowerCase();
  const p = String(pkg || '').toLowerCase();
  if (url) return 'remote-url';
  if (c.includes('npx') || p.startsWith('npm:')) return 'npx';
  if (c.includes('uvx') || p.startsWith('pypi:')) return 'uvx';
  if (c.includes('docker') || p.startsWith('oci:')) return 'oci';
  if (c.includes('python') || c.includes('node') || c.includes('bash') || c.includes('sh')) return 'local-command';
  if (!c && !p) return 'unspecified';
  return 'local-command';
}
function classify({ transport, launcher, trustLevel = '', policy = {}, args = [] }) {
  const stateBinding = String(policy.stateBinding || '').toLowerCase();
  const scopeClass = String(policy.scopeClass || '').toLowerCase();
  const concurrencyPolicy = String(policy.concurrencyPolicy || '').toLowerCase();
  const credentialBinding = String(policy.credentialBinding || '').toLowerCase();
  const trust = String(trustLevel || '').toLowerCase();
  const joinedArgs = args.join(' ').toLowerCase();
  const lockDomains = new Set();

  if (transport === 'sse-legacy') {
    return profile('PX_legacy_compat', 'legacy-disabled', 0, 0, ['legacy-transport']);
  }
  if (trust.includes('browser') || stateBinding.includes('host-desktop') || joinedArgs.includes('playwright')) {
    return profile('PX_forbidden_browser_until_context_isolated', 'session-pool', 2, 1, ['browser-context', 'session']);
  }
  if (scopeClass === 'shared-exclusive' || concurrencyPolicy === 'single-session') {
    return profile('PX_forbidden', 'singleton', 1, 1, ['session']);
  }
  if (credentialBinding && credentialBinding !== 'none') {
    return profile('P2_session_safe', 'credential-session-pool', 4, 1, [`credential:${credentialBinding}`]);
  }
  if (trust.includes('filesystem') || trust.includes('repository') || ['file', 'repo', 'db', 'project'].some((x) => stateBinding.includes(x))) {
    lockDomains.add(trust.includes('repository') || stateBinding.includes('repo') ? 'repo' : 'project');
    if (trust.includes('filesystem') || stateBinding.includes('file')) lockDomains.add('file');
    if (stateBinding.includes('db')) lockDomains.add('db');
    return profile('P3_project_safe', 'project-pool', 4, 1, [...lockDomains]);
  }
  if (trust.includes('network') && transport === 'stdio') {
    return profile('P1_readonly_candidate', 'process-pool', 4, 1, ['credential-or-provider-budget']);
  }
  if (trust.includes('network') || launcher === 'remote-url' || transport === 'streamable-http') {
    return profile('P4_stateless_remote_candidate', 'remote-http-session-pool', 8, 4, ['credential-or-provider-budget']);
  }
  if (concurrencyPolicy === 'multi-reader') {
    return profile('P1_readonly_candidate', 'process-pool', Math.max(2, Number(policy.parallelismLimit || 4)), 1, ['server']);
  }
  if (launcher === 'npx' || launcher === 'uvx' || launcher === 'oci' || launcher === 'local-command') {
    return profile('P0_unknown_stdio', 'process-pool', 2, 1, ['server']);
  }
  return profile('P0_unknown', 'singleton', 1, 1, ['server']);

  function profile(parallelSafetyClass, defaultPoolModel, maxWorkers, maxInFlightPerWorker, locks) {
    return { parallelSafetyClass, defaultPoolModel, maxWorkers, maxInFlightPerWorker, lockDomains: locks };
  }
}
function fromPreset(preset) {
  const transport = normalizeTransport(preset.kind, preset.url);
  const launcher = launcherFor(preset.command, preset.url);
  return { serverId: preset.id, source: 'preset', transport, launcher, trustLevel: preset.trustLevel, policy: {}, args: preset.args || [] };
}
function fromConfigServer(name, value) {
  const policy = value.policy || {};
  const installer = value.installer || {};
  const transport = normalizeTransport(value.transportPreference || value.kind, value.url);
  const launcher = launcherFor(value.command, value.url, installer.installPackage);
  return { serverId: name, source: 'config', transport, launcher, trustLevel: value.trustLevel || '', policy, args: value.args || [] };
}
function collectMcpSettings(root) {
  const out = [];
  const dirs = [join(root, 'mcp_settings.d')];
  for (const dir of dirs) {
    if (!existsSync(dir) || !statSync(dir).isDirectory()) continue;
    for (const file of readdirSync(dir).filter((name) => name.endsWith('.json')).sort()) {
      const data = readJson(join(dir, file), null);
      const servers = data?.mcpServers && typeof data.mcpServers === 'object' ? data.mcpServers : data;
      if (!servers || typeof servers !== 'object') continue;
      for (const [name, server] of Object.entries(servers)) {
        if (!server || typeof server !== 'object') continue;
        out.push({ serverId: name, source: `mcp_settings.d/${file}`, transport: normalizeTransport(server.type, server.url), launcher: launcherFor(server.command, server.url), trustLevel: server.trustLevel || '', policy: server.policy || {}, args: server.args || [] });
      }
    }
  }
  return out;
}
function makeProfile(raw) {
  const classified = classify(raw);
  return {
    serverId: raw.serverId,
    source: raw.source,
    transport: raw.transport,
    launcher: raw.launcher,
    parallelSafetyClass: classified.parallelSafetyClass,
    defaultPoolModel: classified.defaultPoolModel,
    maxWorkers: classified.maxWorkers,
    maxInFlightPerWorker: classified.maxInFlightPerWorker,
    lockDomains: classified.lockDomains,
    evidence: [{ kind: 'static', confidence: 0.45, summary: 'Profile inferred from static config/preset metadata; safe probes and runtime evidence are required before raising trust.' }]
  };
}
function edgeCaseFixtures() {
  return [
    {
      id: 'unknown-stdio-npx',
      raw: { serverId: 'unknown-stdio-npx', source: 'edge-fixture', transport: 'stdio', launcher: 'npx', trustLevel: '', policy: {}, args: ['-y', '@vendor/random-mcp'] },
      expected: { parallelSafetyClass: 'P0_unknown_stdio', defaultPoolModel: 'process-pool', maxInFlightPerWorker: 1 },
      rationale: 'Unknown stdio can scale only through isolated workers; a single worker stays one in-flight until probes pass.'
    },
    {
      id: 'legacy-sse',
      raw: { serverId: 'legacy-sse', source: 'edge-fixture', transport: 'sse-legacy', launcher: 'remote-url', trustLevel: 'network', policy: {}, args: [] },
      expected: { parallelSafetyClass: 'PX_legacy_compat', defaultPoolModel: 'legacy-disabled', maxInFlightPerWorker: 0 },
      rationale: 'Legacy SSE compatibility must not be folded into stable Streamable HTTP scheduling.'
    },
    {
      id: 'remote-streamable-http',
      raw: { serverId: 'remote-streamable-http', source: 'edge-fixture', transport: 'streamable-http', launcher: 'remote-url', trustLevel: 'network', policy: {}, args: [] },
      expected: { parallelSafetyClass: 'P4_stateless_remote_candidate', defaultPoolModel: 'remote-http-session-pool', maxInFlightPerWorker: 4 },
      rationale: 'Remote Streamable HTTP can use session/provider budgets, not local stdio process assumptions.'
    },
    {
      id: 'credential-scoped-api',
      raw: { serverId: 'credential-scoped-api', source: 'edge-fixture', transport: 'stdio', launcher: 'local-command', trustLevel: 'network', policy: { credentialBinding: 'oauth-subject' }, args: [] },
      expected: { parallelSafetyClass: 'P2_session_safe', defaultPoolModel: 'credential-session-pool', maxInFlightPerWorker: 1 },
      rationale: 'Credential identity is a scheduling boundary even when the launcher is local stdio.'
    },
    {
      id: 'project-filesystem-write',
      raw: { serverId: 'project-filesystem-write', source: 'edge-fixture', transport: 'stdio', launcher: 'npx', trustLevel: 'filesystem', policy: { scopeClass: 'project-local', stateBinding: 'file' }, args: [] },
      expected: { parallelSafetyClass: 'P3_project_safe', defaultPoolModel: 'project-pool', maxInFlightPerWorker: 1 },
      rationale: 'Project-local file tools require project/file lock domains; worker concurrency comes from isolation.'
    },
    {
      id: 'repo-git-write',
      raw: { serverId: 'repo-git-write', source: 'edge-fixture', transport: 'stdio', launcher: 'uvx', trustLevel: 'repository', policy: { scopeClass: 'project-local', stateBinding: 'repo' }, args: [] },
      expected: { parallelSafetyClass: 'P3_project_safe', defaultPoolModel: 'project-pool', maxInFlightPerWorker: 1 },
      rationale: 'Git/repository tools can parallelize across repos but must serialize conflicting repo writes.'
    },
    {
      id: 'browser-automation',
      raw: { serverId: 'browser-automation', source: 'edge-fixture', transport: 'stdio', launcher: 'npx', trustLevel: 'browser', policy: { stateBinding: 'host-desktop' }, args: ['@playwright/mcp'] },
      expected: { parallelSafetyClass: 'PX_forbidden_browser_until_context_isolated', defaultPoolModel: 'session-pool', maxInFlightPerWorker: 1 },
      rationale: 'Browser automation needs browser-context/session isolation before parallel scheduling.'
    },
    {
      id: 'shared-exclusive-desktop',
      raw: { serverId: 'shared-exclusive-desktop', source: 'edge-fixture', transport: 'stdio', launcher: 'local-command', trustLevel: 'desktop', policy: { scopeClass: 'shared-exclusive', concurrencyPolicy: 'single-session' }, args: [] },
      expected: { parallelSafetyClass: 'PX_forbidden', defaultPoolModel: 'singleton', maxInFlightPerWorker: 1 },
      rationale: 'Desktop/host-global state stays singleton unless a stronger isolation key is proven.'
    },
    {
      id: 'readonly-stdio-candidate',
      raw: { serverId: 'readonly-stdio-candidate', source: 'edge-fixture', transport: 'stdio', launcher: 'npx', trustLevel: 'network', policy: { concurrencyPolicy: 'multi-reader' }, args: [] },
      expected: { parallelSafetyClass: 'P1_readonly_candidate', defaultPoolModel: 'process-pool', maxInFlightPerWorker: 1 },
      rationale: 'Read-heavy stdio stays one in-flight per worker until safe probes prove higher concurrency.'
    },
    {
      id: 'oci-unknown',
      raw: { serverId: 'oci-unknown', source: 'edge-fixture', transport: 'stdio', launcher: 'oci', trustLevel: '', policy: {}, args: ['docker', 'run', 'example/mcp'] },
      expected: { parallelSafetyClass: 'P0_unknown_stdio', defaultPoolModel: 'process-pool', maxInFlightPerWorker: 1 },
      rationale: 'Container launchers are still untrusted upstream code until classified/probed.'
    }
  ];
}
function buildEdgeCases() {
  return edgeCaseFixtures().map((fixture) => {
    const actual = classify(fixture.raw);
    const locksOk = Array.isArray(actual.lockDomains) && actual.lockDomains.length > 0;
    const expectedOk = Object.entries(fixture.expected).every(([key, value]) => actual[key] === value);
    return { ...fixture, actual, ok: expectedOk && locksOk, locksOk };
  });
}
const root = resolve(argValue('--root', process.cwd()));
const config = readJson(join(root, 'mcpace.config.json'), {});
const presetCatalog = readJson(join(root, 'presets', 'mcp-servers.json'), { presets: [] });
const rawProfiles = [];
for (const [name, value] of Object.entries(config.servers || {})) rawProfiles.push(fromConfigServer(name, value));
for (const preset of presetCatalog.presets || []) rawProfiles.push(fromPreset(preset));
rawProfiles.push(...collectMcpSettings(root));
const profiles = rawProfiles.map(makeProfile);
const edgeCases = buildEdgeCases();
const files = {
  serverModel: readFileSync(join(root, 'src/server/model.rs'), 'utf8'),
  serverLoader: readFileSync(join(root, 'src/server/loader.rs'), 'utf8'),
  clientModel: readFileSync(join(root, 'src/client/model.rs'), 'utf8'),
  clientPlan: readFileSync(join(root, 'src/client/plan.rs'), 'utf8'),
  docsAdaptive: existsSync(join(root, 'docs/adaptive-mcp-orchestration.md')),
  docsEdgeCases: existsSync(join(root, 'docs/adaptive-edge-case-coverage.md')),
  schemaProfile: existsSync(join(root, 'schemas/mcpace-server-profile.schema.json')),
  schemaWorker: existsSync(join(root, 'schemas/mcpace-worker-plan.schema.json')),
};
const checks = [
  { id: 'server-profile-fields', ok: files.serverModel.includes('parallel_safety_class') && files.serverModel.includes('default_pool_model'), detail: 'ServerRecord exposes adaptive profile fields.' },
  { id: 'source-type-normalization', ok: files.serverLoader.includes('sse-legacy') && files.serverLoader.includes('streamable-http'), detail: 'Legacy SSE is separated from stable Streamable HTTP.' },
  { id: 'client-plan-scheduling', ok: files.clientPlan.includes('worker_pool_key') && files.clientPlan.includes('bounded-worker-pool-pending-probe'), detail: 'Client routing plan includes adaptive worker-pool planning and probe-gated fallback.' },
  { id: 'schema-profile', ok: files.schemaProfile, detail: 'Server profile schema exists.' },
  { id: 'schema-worker', ok: files.schemaWorker, detail: 'Worker plan schema exists.' },
  { id: 'docs', ok: files.docsAdaptive, detail: 'Adaptive orchestration architecture doc exists.' },
  { id: 'edge-case-docs', ok: files.docsEdgeCases, detail: 'Adaptive edge-case coverage doc exists.' },
  { id: 'no-legacy-default', ok: profiles.concat(edgeCases.map((x) => x.actual)).every((p) => p.transport !== 'sse-legacy' || p.defaultPoolModel === 'legacy-disabled'), detail: 'Legacy transport is never auto-parallelized.' },
  { id: 'unknown-is-conservative', ok: profiles.concat(edgeCases.map((x) => x.actual)).every((p) => !p.parallelSafetyClass.startsWith('P0_') || p.maxInFlightPerWorker === 1), detail: 'Unknown profiles are maxInFlightPerWorker=1.' },
  { id: 'edge-case-matrix', ok: edgeCases.every((edgeCase) => edgeCase.ok), detail: 'Synthetic edge-case matrix covers unknown, legacy, remote, credential, project, repo, browser, desktop, readonly, and OCI classifications.' },
  { id: 'edge-locks-present', ok: edgeCases.every((edgeCase) => edgeCase.locksOk), detail: 'Every edge-case classification carries at least one lock or scheduling domain.' },
];
const blockers = checks.filter((c) => !c.ok).map((c) => `${c.id}: ${c.detail}`);
const warnings = [];
for (const profile of profiles) {
  if (profile.parallelSafetyClass.startsWith('P0_')) warnings.push(`${profile.serverId}: unknown profile remains conservative until probes pass.`);
  if (profile.parallelSafetyClass.startsWith('PX_')) warnings.push(`${profile.serverId}: high-risk/legacy profile requires explicit policy before parallelism.`);
}
for (const edgeCase of edgeCases) {
  if (!edgeCase.ok) warnings.push(`${edgeCase.id}: expected ${JSON.stringify(edgeCase.expected)} but got ${JSON.stringify(edgeCase.actual)}.`);
}
const report = {
  status: blockers.length ? 'blocked' : 'pass',
  generatedAt: new Date().toISOString(),
  root,
  summary: {
    profileCount: profiles.length,
    edgeCaseCount: edgeCases.length,
    stableCount: profiles.filter((p) => ['P3_project_safe', 'P4_stateless_remote_candidate', 'P1_readonly_candidate'].includes(p.parallelSafetyClass)).length,
    conservativeCount: profiles.filter((p) => p.parallelSafetyClass.startsWith('P0_')).length,
    legacyCount: profiles.filter((p) => p.parallelSafetyClass.includes('legacy')).length,
  },
  profiles,
  edgeCases,
  checks,
  warnings,
  blockers,
};
const write = argValue('--write');
if (write) writeFileSync(write, `${JSON.stringify(report, null, 2)}\n`);
const markdown = argValue('--markdown');
if (markdown) writeMarkdown(markdown, report);
if (has('--json')) console.log(JSON.stringify(report, null, 2));
if (blockers.length) process.exitCode = 1;
