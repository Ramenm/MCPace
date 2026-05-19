#!/usr/bin/env node
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { performance } from 'node:perf_hooks';
import { insertTopK } from './lib/bounded-top-k.mjs';
import { profileFrom } from './lib/mcp-evidence-profile.mjs';
import { deriveProjectName, deriveProjectVersion, repoRoot } from './lib/project-metadata.mjs';

const DEFAULTS = Object.freeze({
  servers: 100,
  tools: 50_000,
  operations: 10_000,
  runs: 7,
  profileRefreshes: 8,
  memoryLimitMiB: 256,
  maxConfigMergeP95Ms: 500,
  maxProfilePerServerUs: 250,
  maxCachedProfilePerServerUs: 120,
  maxToolIndexPerToolUs: 75,
  maxSchedulerPerOperationUs: 75,
  maxRouteLookupPerLookupUs: 40,
});

function parseArgs(argv) {
  const args = {
    ...DEFAULTS,
    json: false,
    strict: false,
    write: 'reports/mcp-overhead-deep-latest.json',
    markdown: 'reports/mcp-overhead-deep-latest.md',
    help: false,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    const readValue = () => {
      const value = argv[index + 1];
      if (!value || value.startsWith('--')) throw new Error(`${token} requires a value`);
      index += 1;
      return value;
    };
    switch (token) {
      case '--json': args.json = true; break;
      case '--strict': args.strict = true; break;
      case '--write': args.write = readValue(); break;
      case '--markdown': args.markdown = readValue(); break;
      case '--no-write': args.write = null; args.markdown = null; break;
      case '--servers': args.servers = positiveInteger(readValue(), token); break;
      case '--tools': args.tools = positiveInteger(readValue(), token); break;
      case '--operations': args.operations = positiveInteger(readValue(), token); break;
      case '--runs': args.runs = positiveInteger(readValue(), token); break;
      case '--profile-refreshes': args.profileRefreshes = positiveInteger(readValue(), token); break;
      case '--memory-limit-mib': args.memoryLimitMiB = positiveNumber(readValue(), token); break;
      case '--max-config-merge-p95-ms': args.maxConfigMergeP95Ms = positiveNumber(readValue(), token); break;
      case '--max-profile-per-server-us': args.maxProfilePerServerUs = positiveNumber(readValue(), token); break;
      case '--max-cached-profile-per-server-us': args.maxCachedProfilePerServerUs = positiveNumber(readValue(), token); break;
      case '--max-tool-index-per-tool-us': args.maxToolIndexPerToolUs = positiveNumber(readValue(), token); break;
      case '--max-scheduler-per-operation-us': args.maxSchedulerPerOperationUs = positiveNumber(readValue(), token); break;
      case '--max-route-lookup-per-lookup-us': args.maxRouteLookupPerLookupUs = positiveNumber(readValue(), token); break;
      case '--help':
      case '-h': args.help = true; break;
      default: throw new Error(`unsupported mcp-overhead-deep-audit argument: ${token}`);
    }
  }
  return args;
}

function positiveInteger(value, label) {
  const parsed = Number.parseInt(value, 10);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) throw new Error(`${label} must be a positive integer`);
  return parsed;
}

function positiveNumber(value, label) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed) || parsed <= 0) throw new Error(`${label} must be a positive number`);
  return parsed;
}

function help() {
  console.log(`Usage: node scripts/mcp-overhead-deep-audit.mjs [--json] [--servers N] [--tools N] [--operations N]

Deep synthetic overhead audit for the automatic/evidence-first MCP hub model.
It never starts random MCP servers, never installs packages, and never sends MCP tool invocation requests.
It measures config fragment parsing, shared evidence-profile inference, cached profile refreshes,
tool route indexing/search, and scheduler lock routing at 100-server scale by default.`);
}

class Rng {
  constructor(seed = 0x5eed1234) { this.state = seed >>> 0; }
  next() { this.state = (Math.imul(this.state, 1664525) + 1013904223) >>> 0; return this.state / 2 ** 32; }
  int(max) { return Math.floor(this.next() * max); }
  pick(values) { return values[this.int(values.length)]; }
}

function stableFingerprint(server) {
  return JSON.stringify({ name: server.name, transport: server.transport, command: server.command, args: server.args, url: server.url, policy: server.policy });
}

function generateServerSpecs(count) {
  const families = [
    'filesystem', 'memory', 'sqlite', 'git', 'remote-http', 'remote-http-stateless',
    'credential-api', 'browser', 'shell', 'docs-fetch', 'time', 'unknown-npx',
  ];
  const specs = [];
  for (let index = 0; index < count; index += 1) {
    const family = families[index % families.length];
    const project = index % 7;
    const base = { serverId: `${family}-${String(index).padStart(3, '0')}`, name: `${family}-${String(index).padStart(3, '0')}`, enabled: false, args: [], policy: {} };
    let spec;
    switch (family) {
      case 'filesystem':
        spec = { ...base, transport: 'stdio', command: 'npx', args: ['@modelcontextprotocol/server-filesystem', `/workspace/project-${project}`] };
        break;
      case 'memory':
        spec = { ...base, transport: 'stdio', command: 'npx', args: ['@modelcontextprotocol/server-memory'] };
        break;
      case 'sqlite':
        spec = { ...base, transport: 'stdio', command: 'uvx', args: ['mcp-server-sqlite', '--db-path', `/workspace/project-${project}/data.sqlite`] };
        break;
      case 'git':
        spec = { ...base, transport: 'stdio', command: 'uvx', args: ['mcp-server-git', '--repository', `/workspace/project-${project}`] };
        break;
      case 'remote-http':
        spec = { ...base, transport: 'streamable-http', url: `https://mcp${index}.example.test/mcp` };
        break;
      case 'remote-http-stateless':
        spec = { ...base, transport: 'streamable-http', url: `https://docs${index}.example.test/mcp`, policy: { stateless: true, stateBinding: 'none', concurrencyPolicy: 'multi-reader', parallelismLimit: 8 } };
        break;
      case 'credential-api':
        spec = { ...base, transport: 'stdio', command: 'npx', args: [`vendor-${index}-slack-api-mcp`, '--token-env', 'SLACK_TOKEN'] };
        break;
      case 'browser':
        spec = { ...base, transport: 'stdio', command: 'npx', args: ['@playwright/mcp', '--browser', 'chromium'] };
        break;
      case 'shell':
        spec = { ...base, transport: 'stdio', command: 'node', args: ['command-runner-mcp.js', '--exec'] };
        break;
      case 'docs-fetch':
        spec = { ...base, transport: 'stdio', command: 'npx', args: ['@upstash/context7-mcp'] };
        break;
      case 'time':
        spec = { ...base, transport: 'stdio', command: 'uvx', args: ['mcp-server-time'], policy: { stateless: true, stateBinding: 'none', concurrencyPolicy: 'multi-reader', parallelismLimit: 4 } };
        break;
      default:
        spec = { ...base, transport: 'stdio', command: 'npx', args: [`random-mcp-package-${index}`] };
        break;
    }
    spec._fingerprint = stableFingerprint(spec);
    specs.push(spec);
  }
  return specs;
}

function generateFragments(specs) {
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-overhead-fragments-'));
  for (const spec of specs) {
    fs.writeFileSync(path.join(tempDir, `${spec.name}.json`), JSON.stringify({
      mcpServers: {
        [spec.name]: {
          command: spec.command,
          args: spec.args,
          url: spec.url,
          transport: spec.transport,
          enabled: spec.enabled,
          policy: spec.policy,
        },
      },
    }), 'utf8');
  }
  return tempDir;
}

function mergeFragments(fragmentDir) {
  const merged = { mcpServers: {} };
  const files = fs.readdirSync(fragmentDir).filter((name) => name.endsWith('.json')).sort();
  for (const file of files) {
    const parsed = JSON.parse(fs.readFileSync(path.join(fragmentDir, file), 'utf8'));
    Object.assign(merged.mcpServers, parsed.mcpServers || {});
  }
  return { serverCount: Object.keys(merged.mcpServers).length, fragmentCount: files.length };
}

function cleanupDir(dir) {
  try { fs.rmSync(dir, { recursive: true, force: true }); } catch { /* ignore */ }
}

function buildProfilesCold(specs, refreshes = 1) {
  let profiles = [];
  for (let refresh = 0; refresh < refreshes; refresh += 1) profiles = specs.map((spec) => profileFrom(spec, 'synthetic-overhead'));
  return summarizeProfiles(profiles, refreshes);
}

function warmProfileCache(specs) {
  const cache = new Map();
  for (const spec of specs) cache.set(spec._fingerprint, Object.freeze(profileFrom(spec, 'synthetic-overhead')));
  return cache;
}

function buildProfilesCached(specs, refreshes, cache) {
  let profiles = [];
  let hits = 0;
  for (let refresh = 0; refresh < refreshes; refresh += 1) {
    profiles = [];
    for (const spec of specs) {
      const profile = cache.get(spec._fingerprint);
      if (!profile) throw new Error(`profile cache was not warmed for ${spec.serverId}`);
      hits += 1;
      profiles.push(profile);
    }
  }
  return { ...summarizeProfiles(profiles, refreshes), cache: { size: cache.size, hits, misses: cache.size, hotPathOnly: true } };
}

function summarizeProfiles(profiles, refreshes) {
  const counts = new Map();
  for (const profile of profiles) counts.set(profile.parallelSafetyClass, (counts.get(profile.parallelSafetyClass) || 0) + 1);
  return {
    profileCount: profiles.length,
    refreshes,
    safetyClassCounts: Object.fromEntries([...counts.entries()].sort()),
    statelessCount: profiles.filter((profile) => profile.stateless).length,
    statefulCount: profiles.filter((profile) => profile.stateful).length,
    reviewRequiredCount: profiles.filter((profile) => profile.parallelSafetyClass.startsWith('P0_') || profile.parallelSafetyClass.startsWith('PX_')).length,
  };
}

function poolModel(profile) {
  return profile.defaultPoolModel || profile.poolModel || 'process-pool';
}

function lockTemplateForDomain(domain) {
  const normalized = String(domain || 'server');
  if (normalized.startsWith('credential')) return 'credential:{credentialProfile}';
  switch (normalized) {
    case 'file': return 'file:{projectRoot}';
    case 'project': return 'project:{projectRoot}';
    case 'repo': return 'repo:{repoRoot}';
    case 'db':
    case 'database': return 'db:{dbPath}';
    case 'context-store': return 'context-store:{sessionId}:{contextId}';
    case 'session': return 'session:{sessionId}';
    case 'transport-session': return 'transport-session:{transportSessionId}';
    case 'tenant': return 'tenant:{tenantId}';
    case 'provider-budget': return 'provider-budget:{providerBudgetKey}';
    case 'browser-context': return 'browser-context:{browserContextId}';
    case 'host-session': return 'host-session:{hostSessionId}';
    case 'legacy-transport': return 'legacy-transport:{serverId}';
    default: return `${normalized}:{serverId}`;
  }
}

function makeWorkerPlan(profile) {
  const disabled = profile.parallelSafetyClass.startsWith('PX_') || poolModel(profile) === 'legacy-disabled';
  return {
    serverId: profile.serverId,
    parallelSafetyClass: profile.parallelSafetyClass,
    poolModel: poolModel(profile),
    maxWorkers: disabled ? 0 : Math.max(1, profile.maxWorkers),
    maxInFlightPerWorker: disabled ? 0 : Math.max(1, profile.maxInFlightPerWorker),
    lockTemplates: (profile.lockDomains || ['server']).map(lockTemplateForDomain),
    requiresConsent: profile.parallelSafetyClass.startsWith('P0_') || profile.parallelSafetyClass.startsWith('PX_'),
  };
}

function materialize(template, op) {
  return template
    .replaceAll('{serverId}', op.serverId)
    .replaceAll('{projectRoot}', op.projectRoot)
    .replaceAll('{repoRoot}', op.repoRoot)
    .replaceAll('{dbPath}', op.dbPath)
    .replaceAll('{sessionId}', op.sessionId)
    .replaceAll('{contextId}', op.contextId)
    .replaceAll('{transportSessionId}', op.transportSessionId)
    .replaceAll('{credentialProfile}', op.credentialProfile)
    .replaceAll('{tenantId}', op.tenantId)
    .replaceAll('{providerBudgetKey}', op.providerBudgetKey)
    .replaceAll('{browserContextId}', op.browserContextId)
    .replaceAll('{hostSessionId}', op.hostSessionId);
}

function workerKey(profile, op) {
  switch (poolModel(profile)) {
    case 'project-pool': return `${profile.serverId}:project:${op.projectRoot}`;
    case 'remote-http-session-pool': return `${profile.serverId}:transport:${op.transportSessionId}:cred:${op.credentialProfile}`;
    case 'remote-http-shared-pool': return `${profile.serverId}:provider:${op.providerBudgetKey}`;
    case 'credential-session-pool': return `${profile.serverId}:cred:${op.credentialProfile}:tenant:${op.tenantId}`;
    case 'session-pool': return `${profile.serverId}:browser:${op.browserContextId}:host:${op.hostSessionId}`;
    case 'singleton': return `${profile.serverId}:singleton`;
    default: return `${profile.serverId}:process`;
  }
}

function toolFamily(toolIndex) {
  if (toolIndex % 17 === 0) return 'shared_lookup';
  return toolIndex % 10 < 7 ? ['read', 'search', 'list', 'describe'][toolIndex % 4] : ['write', 'delete', 'update'][toolIndex % 3];
}

function syntheticTool(server, serverIndex, toolIndex) {
  const family = toolFamily(toolIndex);
  const readOnly = toolIndex % 10 < 7 || family === 'shared_lookup';
  return {
    server,
    name: `${family}_${toolIndex}`,
    qualifiedName: `${server}.${family}_${toolIndex}`,
    description: `${readOnly ? 'Read-only discovery' : 'Mutating state'} tool ${toolIndex} for ${server} over profile ${serverIndex}`,
    readOnly,
  };
}

function scoreTool(tool, terms) {
  const text = `${tool.server} ${tool.name} ${tool.qualifiedName} ${tool.description}`.toLowerCase();
  let score = 0;
  for (const term of terms) {
    if (tool.name.startsWith(term)) score += 40;
    if (tool.qualifiedName.includes(term)) score += 20;
    if (text.includes(term)) score += 1;
  }
  return score;
}

function buildToolIndex(specs, toolCount, lookupCount) {
  const index = new Map();
  const byUnqualified = new Map();
  const top = [];
  const queryTerms = ['read', 'search', 'docs'];
  const perServerBase = Math.floor(toolCount / specs.length);
  const remainder = toolCount % specs.length;
  let collisions = 0;
  let readOnlyCount = 0;
  let mutatingCount = 0;
  for (let serverIndex = 0; serverIndex < specs.length; serverIndex += 1) {
    const spec = specs[serverIndex];
    const count = perServerBase + (serverIndex < remainder ? 1 : 0);
    for (let toolIndex = 0; toolIndex < count; toolIndex += 1) {
      const tool = syntheticTool(spec.name, serverIndex, toolIndex);
      if (tool.readOnly) readOnlyCount += 1;
      else mutatingCount += 1;
      index.set(tool.qualifiedName, { server: spec.name, tool: tool.name, readOnly: tool.readOnly });
      const bucket = byUnqualified.get(tool.name);
      if (bucket) { bucket.count += 1; collisions += 1; }
      else byUnqualified.set(tool.name, { count: 1, first: tool.qualifiedName });
      const score = scoreTool(tool, queryTerms);
      if (score > 0) insertTopK(top, { score, key: tool.qualifiedName }, 32);
    }
  }
  let lookupHits = 0;
  const rng = new Rng(0x14ee1000);
  const lookupStarted = performance.now();
  for (let indexNumber = 0; indexNumber < lookupCount; indexNumber += 1) {
    const serverIndex = rng.int(specs.length);
    const count = perServerBase + (serverIndex < remainder ? 1 : 0);
    const toolIndex = count > 0 ? rng.int(count) : 0;
    const key = `${specs[serverIndex].name}.${toolFamily(toolIndex)}_${toolIndex}`;
    if (index.get(key)) lookupHits += 1;
  }
  const lookupElapsedMs = performance.now() - lookupStarted;
  return {
    toolCount,
    indexSize: index.size,
    unqualifiedNameCount: byUnqualified.size,
    unqualifiedCollisions: collisions,
    retainedSearchCandidates: top.length,
    lookupCount,
    lookupHits,
    lookupElapsedMs: round(lookupElapsedMs),
    lookupPerLookupUs: round((lookupElapsedMs * 1000) / Math.max(1, lookupCount)),
    readOnlyCount,
    mutatingCount,
  };
}

function simulateScheduler(profiles, operationCount) {
  const rng = new Rng(0x51ced00d);
  const plans = profiles.map(makeWorkerPlan);
  const profileById = new Map(profiles.map((profile) => [profile.serverId, profile]));
  const contexts = {
    projects: ['/repo/a', '/repo/b', '/repo/c', '/repo/d'],
    sessions: ['chat-a', 'chat-b', 'chat-c', 'chat-d'],
    contextIds: ['ctx-1', 'ctx-2', 'ctx-3'],
    transportSessions: ['ts-a', 'ts-b', 'ts-c', 'ts-d'],
    credentials: ['anon', 'user-a', 'user-b', 'user-c'],
    tenants: ['tenant-a', 'tenant-b'],
    providers: ['docs', 'search', 'maps'],
    browserContexts: ['browser-a', 'browser-b'],
  };
  const locks = new Set();
  const workers = new Map();
  let started = 0;
  let reviewBlocked = 0;
  let conflictBlocked = 0;
  let maxInFlightBlocked = 0;
  let disabledBlocked = 0;
  for (let index = 0; index < operationCount; index += 1) {
    const plan = plans[rng.int(plans.length)];
    if (plan.maxWorkers === 0) { disabledBlocked += 1; continue; }
    if (plan.requiresConsent && rng.next() < 0.9) { reviewBlocked += 1; continue; }
    const projectRoot = rng.pick(contexts.projects);
    const op = {
      serverId: plan.serverId,
      projectRoot,
      repoRoot: `${projectRoot}/.git`,
      dbPath: `${projectRoot}/data.sqlite`,
      sessionId: rng.pick(contexts.sessions),
      contextId: rng.pick(contexts.contextIds),
      transportSessionId: rng.pick(contexts.transportSessions),
      credentialProfile: rng.pick(contexts.credentials),
      tenantId: rng.pick(contexts.tenants),
      providerBudgetKey: rng.pick(contexts.providers),
      browserContextId: rng.pick(contexts.browserContexts),
      hostSessionId: 'host-1',
    };
    const opLocks = plan.lockTemplates.map((template) => materialize(template, op));
    if (opLocks.some((lock) => locks.has(lock))) { conflictBlocked += 1; continue; }
    const key = workerKey(profileById.get(plan.serverId) || { defaultPoolModel: 'process-pool', serverId: plan.serverId }, op);
    const count = workers.get(key) || 0;
    if (count >= plan.maxInFlightPerWorker) { maxInFlightBlocked += 1; continue; }
    for (const lock of opLocks) locks.add(lock);
    workers.set(key, count + 1);
    // Race-condition audits cover timed overlap. This audit measures route/lock accounting overhead.
    for (const lock of opLocks) locks.delete(lock);
    workers.set(key, Math.max(0, (workers.get(key) || 0) - 1));
    started += 1;
  }
  return { operationCount, started, reviewBlocked, conflictBlocked, maxInFlightBlocked, disabledBlocked };
}

function readMassSurveySafety() {
  const reportPath = path.join(repoRoot, 'reports/mcp-mass-package-survey-latest.json');
  if (!fs.existsSync(reportPath)) return { status: 'missing', path: 'reports/mcp-mass-package-survey-latest.json', packageCount: 0, safe: false };
  const report = JSON.parse(fs.readFileSync(reportPath, 'utf8'));
  const safety = report.safety || {};
  return {
    status: report.status,
    mode: report.mode,
    path: 'reports/mcp-mass-package-survey-latest.json',
    packageCount: report.summary?.packageCount ?? report.packages?.length ?? 0,
    downloadedTarballs: report.summary?.downloadedTarballs ?? 0,
    safe: safety.executesThirdPartyPackages === false && safety.startsMcpServers === false && safety.callsMcpTools === false && safety.packageInstallScriptsAllowed === false && safety.defaultServerEnablement === false,
  };
}

function maybeGc() { if (typeof global.gc === 'function') global.gc(); }
function percentile(values, pct) { const sorted = [...values].sort((a, b) => a - b); const index = Math.max(0, Math.min(sorted.length - 1, Math.ceil((pct / 100) * sorted.length) - 1)); return sorted[index] ?? null; }
function median(values) { return percentile(values, 50); }
function round(value, digits = 3) { return Number(Number(value).toFixed(digits)); }

function measure(name, runs, fn) {
  for (let warmup = 0; warmup < Math.min(2, runs); warmup += 1) fn();
  const durations = [];
  const heapDeltas = [];
  let last = null;
  for (let run = 0; run < runs; run += 1) {
    maybeGc();
    const heapBefore = process.memoryUsage().heapUsed;
    const started = performance.now();
    last = fn();
    const durationMs = performance.now() - started;
    durations.push(durationMs);
    heapDeltas.push((process.memoryUsage().heapUsed - heapBefore) / 1024 / 1024);
  }
  return {
    name,
    runs,
    minMs: round(Math.min(...durations)),
    medianMs: round(median(durations)),
    p95Ms: round(percentile(durations, 95)),
    maxMs: round(Math.max(...durations)),
    heapDeltaMiBMedian: round(median(heapDeltas)),
    heapDeltaMiBP95: round(percentile(heapDeltas, 95)),
    last,
  };
}

function check(id, ok, detail, evidence = {}) {
  return { id, status: ok ? 'pass' : 'fail', ok: Boolean(ok), detail, evidence };
}

function makeReport(args) {
  const specs = generateServerSpecs(args.servers);
  const fragmentDir = generateFragments(specs);
  let configMerge;
  try { configMerge = measure('config-fragment-merge', args.runs, () => mergeFragments(fragmentDir)); }
  finally { cleanupDir(fragmentDir); }

  const profileCold = measure('profile-inference-cold-refreshes', args.runs, () => buildProfilesCold(specs, args.profileRefreshes));
  const profileCache = warmProfileCache(specs);
  const profileCached = measure('profile-inference-cached-hot-refreshes', args.runs, () => buildProfilesCached(specs, args.profileRefreshes, profileCache));
  const profiles = specs.map((spec) => profileFrom(spec, 'synthetic-overhead'));
  const workerPlanBuild = measure('worker-plan-build', args.runs, () => profiles.map(makeWorkerPlan));
  const toolIndex = measure('tool-index-search-route', args.runs, () => buildToolIndex(specs, args.tools, Math.min(args.operations, args.tools)));
  const scheduler = measure('scheduler-lock-routing', args.runs, () => simulateScheduler(profiles, args.operations));
  const massSurveySafety = readMassSurveySafety();

  const profileColdPerServerUs = (profileCold.p95Ms * 1000) / Math.max(1, args.servers * args.profileRefreshes);
  const profileCachedPerServerUs = (profileCached.p95Ms * 1000) / Math.max(1, args.servers * args.profileRefreshes);
  const toolIndexPerToolUs = (toolIndex.p95Ms * 1000) / Math.max(1, args.tools);
  const routeLookupPerLookupUs = toolIndex.last.lookupPerLookupUs;
  const schedulerPerOperationUs = (scheduler.p95Ms * 1000) / Math.max(1, args.operations);
  const maxHeapDeltaMiB = Math.max(...[configMerge, profileCold, profileCached, workerPlanBuild, toolIndex, scheduler].map((entry) => Math.max(0, entry.heapDeltaMiBP95)));

  const checks = [
    check('config-fragment-merge-p95-budget', configMerge.p95Ms <= args.maxConfigMergeP95Ms, `p95 ${configMerge.p95Ms}ms <= ${args.maxConfigMergeP95Ms}ms`, { p95Ms: configMerge.p95Ms }),
    check('cold-profile-inference-per-server-budget', profileColdPerServerUs <= args.maxProfilePerServerUs, `p95 ${round(profileColdPerServerUs)}us/server <= ${args.maxProfilePerServerUs}us`, { profileColdPerServerUs: round(profileColdPerServerUs) }),
    check('cached-profile-inference-per-server-budget', profileCachedPerServerUs <= args.maxCachedProfilePerServerUs, `p95 ${round(profileCachedPerServerUs)}us/server <= ${args.maxCachedProfilePerServerUs}us`, { profileCachedPerServerUs: round(profileCachedPerServerUs) }),
    check('cached-refresh-actually-hits', profileCached.last.cache.hits > 0 && profileCached.last.cache.misses === profileCached.last.cache.size, `hits=${profileCached.last.cache.hits}, misses=${profileCached.last.cache.misses}`, profileCached.last.cache),
    check('worker-plan-build-materializes-all-servers', workerPlanBuild.last.length === args.servers, `${workerPlanBuild.last.length}/${args.servers} plans`, { planCount: workerPlanBuild.last.length }),
    check('tool-index-build-per-tool-budget', toolIndexPerToolUs <= args.maxToolIndexPerToolUs, `p95 ${round(toolIndexPerToolUs)}us/tool <= ${args.maxToolIndexPerToolUs}us`, { toolIndexPerToolUs: round(toolIndexPerToolUs) }),
    check('route-lookups-hit-and-stay-cheap', toolIndex.last.lookupHits === toolIndex.last.lookupCount && routeLookupPerLookupUs <= args.maxRouteLookupPerLookupUs, `hits=${toolIndex.last.lookupHits}/${toolIndex.last.lookupCount}, ${routeLookupPerLookupUs}us/lookup <= ${args.maxRouteLookupPerLookupUs}us`, { routeLookupPerLookupUs }),
    check('tool-search-retains-bounded-candidates', toolIndex.last.retainedSearchCandidates <= 32, `${toolIndex.last.retainedSearchCandidates} retained`, { retainedSearchCandidates: toolIndex.last.retainedSearchCandidates }),
    check('scheduler-per-operation-budget', schedulerPerOperationUs <= args.maxSchedulerPerOperationUs, `p95 ${round(schedulerPerOperationUs)}us/op <= ${args.maxSchedulerPerOperationUs}us`, { schedulerPerOperationUs: round(schedulerPerOperationUs) }),
    check('scheduler-keeps-review-gates-active', scheduler.last.reviewBlocked > 0 && scheduler.last.disabledBlocked > 0, `review=${scheduler.last.reviewBlocked}, disabled=${scheduler.last.disabledBlocked}`, scheduler.last),
    check('heap-delta-budget', maxHeapDeltaMiB <= args.memoryLimitMiB, `max p95 heap delta ${round(maxHeapDeltaMiB)}MiB <= ${args.memoryLimitMiB}MiB`, { maxHeapDeltaMiB: round(maxHeapDeltaMiB) }),
    check('mass-survey-safety-proof-present', massSurveySafety.safe && massSurveySafety.packageCount >= 100, `mass survey ${massSurveySafety.status}, packages=${massSurveySafety.packageCount}, safe=${massSurveySafety.safe}`, massSurveySafety),
  ];

  return {
    schema: 'mcpace.mcpOverheadDeepAudit.v1',
    status: checks.every((entry) => entry.ok) ? 'pass' : 'fail',
    generatedAt: new Date().toISOString(),
    project: { name: deriveProjectName(), version: deriveProjectVersion() },
    environment: { node: process.version, platform: process.platform, arch: process.arch, availableParallelism: os.availableParallelism?.() || os.cpus().length || 1 },
    scenario: { servers: args.servers, tools: args.tools, operations: args.operations, runs: args.runs, profileRefreshes: args.profileRefreshes, memoryLimitMiB: args.memoryLimitMiB },
    safety: { syntheticOnly: true, startsMcpServers: false, callsMcpTools: false, executesThirdPartyPackages: false, installsPackages: false },
    summary: {
      configMergeP95Ms: configMerge.p95Ms,
      profileColdPerServerUs: round(profileColdPerServerUs),
      profileCachedPerServerUs: round(profileCachedPerServerUs),
      toolIndexPerToolUs: round(toolIndexPerToolUs),
      routeLookupPerLookupUs,
      schedulerPerOperationUs: round(schedulerPerOperationUs),
      maxHeapDeltaMiB: round(maxHeapDeltaMiB),
      massSurveyPackages: massSurveySafety.packageCount,
    },
    benchmarks: { configMerge, profileCold, profileCached, workerPlanBuild, toolIndex, scheduler },
    massSurveySafety,
    checks,
    blockers: checks.filter((entry) => !entry.ok).map((entry) => `${entry.id}: ${entry.detail}`),
  };
}

function reportOutputPath(target) {
  return path.isAbsolute(target) ? target : path.join(repoRoot, target);
}

function writeReport(report, args) {
  if (args.write) {
    const output = reportOutputPath(args.write);
    fs.mkdirSync(path.dirname(output), { recursive: true });
    fs.writeFileSync(output, `${JSON.stringify(report, null, 2)}\n`);
  }
  if (args.markdown) {
    const output = reportOutputPath(args.markdown);
    fs.mkdirSync(path.dirname(output), { recursive: true });
    fs.writeFileSync(output, renderMarkdown(report));
  }
}

function renderMarkdown(report) {
  const lines = [
    '# MCP overhead deep audit',
    '',
    `Generated: ${report.generatedAt}`,
    `Status: **${report.status}**`,
    `Project: ${report.project.name} ${report.project.version}`,
    `Node: ${report.environment.node} on ${report.environment.platform}/${report.environment.arch}`,
    '',
    '## Scenario',
    '',
    `- Servers: ${report.scenario.servers}`,
    `- Tools: ${report.scenario.tools}`,
    `- Operations: ${report.scenario.operations}`,
    `- Runs: ${report.scenario.runs}`,
    `- Profile refreshes per run: ${report.scenario.profileRefreshes}`,
    '',
    '## Summary',
    '',
    `- Config merge p95: ${report.summary.configMergeP95Ms} ms`,
    `- Cold shared profile inference p95/server: ${report.summary.profileColdPerServerUs} µs`,
    `- Cached shared profile refresh p95/server: ${report.summary.profileCachedPerServerUs} µs`,
    `- Tool index/search p95/tool: ${report.summary.toolIndexPerToolUs} µs`,
    `- Route lookup: ${report.summary.routeLookupPerLookupUs} µs/lookup`,
    `- Scheduler lock routing p95/op: ${report.summary.schedulerPerOperationUs} µs`,
    `- Max p95 heap delta: ${report.summary.maxHeapDeltaMiB} MiB`,
    `- Mass package survey packages: ${report.summary.massSurveyPackages}`,
    '',
    '## Checks',
    '',
    '| Check | Status | Detail |',
    '|---|---:|---|',
  ];
  for (const check of report.checks) lines.push(`| ${check.id} | ${check.status} | ${String(check.detail).replace(/\|/g, '\\|')} |`);
  if (report.blockers.length) {
    lines.push('', '## Blockers');
    for (const blocker of report.blockers) lines.push(`- ${blocker}`);
  }
  return `${lines.join('\n')}\n`;
}

function main() {
  try {
    const args = parseArgs(process.argv.slice(2));
    if (args.help) { help(); return; }
    const report = makeReport(args);
    writeReport(report, args);
    if (args.json) console.log(JSON.stringify(report, null, 2));
    if (args.strict && report.status !== 'pass') process.exitCode = 1;
  } catch (error) {
    console.error(error?.message || String(error));
    process.exitCode = 1;
  }
}

main();
