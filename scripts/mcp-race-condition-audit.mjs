#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { repoRoot, deriveProjectName, deriveProjectVersion } from './lib/project-metadata.mjs';

function parseArgs(argv) {
  const args = {
    json: false,
    strict: false,
    iterations: 3000,
    seeds: 5,
    write: path.join(repoRoot, 'reports/mcp-race-condition-audit-latest.json'),
    markdown: path.join(repoRoot, 'reports/mcp-race-condition-audit-latest.md'),
    help: false,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    const read = () => {
      const value = argv[index + 1];
      if (!value || value.startsWith('--')) throw new Error(`${token} requires a value`);
      index += 1;
      return value;
    };
    if (token === '--json') args.json = true;
    else if (token === '--strict') args.strict = true;
    else if (token === '--iterations') args.iterations = boundedInt(read(), token, 50, 200_000);
    else if (token === '--seeds') args.seeds = boundedInt(read(), token, 1, 100);
    else if (token === '--write') args.write = path.resolve(read());
    else if (token === '--markdown') args.markdown = path.resolve(read());
    else if (token === '--no-write') { args.write = null; args.markdown = null; }
    else if (token === '-h' || token === '--help') args.help = true;
    else throw new Error(`unsupported mcp-race-condition-audit argument: ${token}`);
  }
  return args;
}

function boundedInt(value, label, min, max) {
  const parsed = Number.parseInt(value, 10);
  if (!Number.isSafeInteger(parsed) || parsed < min || parsed > max) throw new Error(`${label} must be an integer in [${min}, ${max}]`);
  return parsed;
}

function help() {
  console.log('Usage: node scripts/mcp-race-condition-audit.mjs [--iterations 3000] [--seeds 5] [--json] [--strict]');
}

function nowNs() { return process.hrtime.bigint(); }
function msSince(start) { return Number(nowNs() - start) / 1_000_000; }
function round(value, places = 3) { const f = 10 ** places; return Math.round(value * f) / f; }

class Rng {
  constructor(seed = 0xace55) { this.s = seed >>> 0; }
  next() { this.s = (Math.imul(this.s, 1664525) + 1013904223) >>> 0; return this.s / 2 ** 32; }
  int(n) { return Math.floor(this.next() * n); }
  pick(values) { return values[this.int(values.length)]; }
}

const PROFILES = Object.freeze([
  { id: 'unknown-npx', safety: 'P0_unknown_stdio', pool: 'process', locks: ['server:{serverId}'], maxInFlight: 1, reviewGate: true },
  { id: 'memory', safety: 'P2_session_safe', pool: 'session', locks: ['session:{sessionId}:ctx:{contextId}', 'context-store:{sessionId}'], maxInFlight: 1 },
  { id: 'filesystem', safety: 'P3_project_safe', pool: 'project', locks: ['file:{projectRoot}', 'project:{projectRoot}'], maxInFlight: 1 },
  { id: 'git', safety: 'P3_project_safe', pool: 'project', locks: ['repo:{repoRoot}', 'project:{projectRoot}'], maxInFlight: 1 },
  { id: 'sqlite', safety: 'P3_project_safe', pool: 'project', locks: ['db:{dbPath}', 'project:{projectRoot}'], maxInFlight: 1 },
  { id: 'remote-http-session', safety: 'P2_session_safe', pool: 'remote-session', locks: ['transport-session:{transportSessionId}', 'credential:{credentialProfile}'], maxInFlight: 1 },
  { id: 'remote-http-stateless', safety: 'P4_stateless_remote_candidate', pool: 'remote-shared', locks: ['provider-budget:{providerBudgetKey}'], maxInFlight: 4 },
  { id: 'credential-api', safety: 'P2_session_safe', pool: 'credential-session', locks: ['credential:{credentialProfile}', 'tenant:{tenantId}'], maxInFlight: 1 },
  { id: 'browser', safety: 'PX_forbidden_browser_until_context_isolated', pool: 'browser', locks: ['browser-context:{browserContextId}', 'host-session:{hostSessionId}'], maxInFlight: 1, reviewGate: true },
]);

const CONTEXT = Object.freeze({
  sessions: ['chat-a', 'chat-b', 'chat-c', 'chat-d', 'chat-e'],
  contexts: ['ctx-1', 'ctx-2', 'ctx-3'],
  projects: ['/p/a', '/p/b', '/p/c'],
  repos: ['/p/a/.git', '/p/b/.git', '/p/c/.git'],
  dbs: ['/p/a/db.sqlite', '/p/b/db.sqlite', '/p/c/db.sqlite'],
  transport: ['ts-a', 'ts-b', 'ts-c', 'ts-d'],
  creds: ['anon', 'user-a', 'user-b', 'user-c'],
  tenants: ['t-a', 't-b', 't-c'],
  providers: ['docs', 'search', 'maps'],
  browsers: ['b-a', 'b-b', 'b-c'],
  hosts: ['host-1'],
  clients: ['claude', 'cursor', 'vscode', 'codex', 'windsurf'],
});

function workerKey(profile, op) {
  switch (profile.pool) {
    case 'project': return `${profile.id}:project:${op.projectRoot}`;
    case 'session': return `${profile.id}:session:${op.sessionId}:ctx:${op.contextId}`;
    case 'remote-session': return `${profile.id}:transport:${op.transportSessionId}:cred:${op.credentialProfile}`;
    case 'remote-shared': return `${profile.id}:provider:${op.providerBudgetKey}`;
    case 'credential-session': return `${profile.id}:cred:${op.credentialProfile}:tenant:${op.tenantId}`;
    case 'browser': return `${profile.id}:browser:${op.browserContextId}:host:${op.hostSessionId}`;
    default: return `${profile.id}:process`;
  }
}

function materialize(template, op) {
  return template
    .replaceAll('{serverId}', op.serverId)
    .replaceAll('{sessionId}', op.sessionId)
    .replaceAll('{contextId}', op.contextId)
    .replaceAll('{projectRoot}', op.projectRoot)
    .replaceAll('{repoRoot}', op.repoRoot)
    .replaceAll('{dbPath}', op.dbPath)
    .replaceAll('{transportSessionId}', op.transportSessionId)
    .replaceAll('{credentialProfile}', op.credentialProfile)
    .replaceAll('{tenantId}', op.tenantId)
    .replaceAll('{providerBudgetKey}', op.providerBudgetKey)
    .replaceAll('{browserContextId}', op.browserContextId)
    .replaceAll('{hostSessionId}', op.hostSessionId);
}

function makeOperation(profile, rng, index) {
  const projectIndex = rng.int(CONTEXT.projects.length);
  const operation = {
    id: `op-${index}`,
    serverId: profile.id,
    profile,
    sessionId: rng.pick(CONTEXT.sessions),
    contextId: rng.pick(CONTEXT.contexts),
    projectRoot: CONTEXT.projects[projectIndex],
    repoRoot: CONTEXT.repos[projectIndex],
    dbPath: CONTEXT.dbs[projectIndex],
    transportSessionId: rng.pick(CONTEXT.transport),
    credentialProfile: rng.pick(CONTEXT.creds),
    tenantId: rng.pick(CONTEXT.tenants),
    providerBudgetKey: rng.pick(CONTEXT.providers),
    browserContextId: rng.pick(CONTEXT.browsers),
    hostSessionId: rng.pick(CONTEXT.hosts),
    clientId: rng.pick(CONTEXT.clients),
    ready: rng.int(250),
    dur: 1 + rng.int(7),
    enabled: rng.next() > 0.4,
    knownTool: rng.next() > 0.1,
    reviewApproved: rng.next() > 0.9,
  };
  operation.workerKey = workerKey(profile, operation);
  operation.locks = profile.locks.map((template) => materialize(template, operation));
  return operation;
}

function assertActiveInvariants(active, tick, violations) {
  const locks = new Map();
  const workers = new Map();
  for (const item of active) {
    for (const lock of item.op.locks) {
      const previous = locks.get(lock);
      if (previous) violations.push(`lock overlap at tick ${tick}: ${lock} owned by ${previous} and ${item.op.id}`);
      locks.set(lock, item.op.id);
    }
    const count = (workers.get(item.op.workerKey) || 0) + 1;
    workers.set(item.op.workerKey, count);
    if (count > item.op.profile.maxInFlight) {
      violations.push(`worker maxInFlight exceeded at tick ${tick}: ${item.op.workerKey} count=${count} limit=${item.op.profile.maxInFlight}`);
    }
    if (!item.op.enabled) violations.push(`disabled operation started: ${item.op.id}`);
    if (!item.op.knownTool) violations.push(`unknown-tool operation started: ${item.op.id}`);
    if (item.op.profile.reviewGate && !item.op.reviewApproved) violations.push(`review-gated operation started without approval: ${item.op.id}`);
  }
}

function simulate(iterations, seed) {
  const startedNs = nowNs();
  const rng = new Rng(seed);
  const all = Array.from({ length: iterations }, (_, index) => makeOperation(rng.pick(PROFILES), rng, index))
    .sort((a, b) => a.ready - b.ready || a.id.localeCompare(b.id));
  const waiting = [];
  const active = [];
  const lockOwners = new Map();
  const workerCounts = new Map();
  const events = [];
  const violations = [];
  const profileCoverage = new Set();
  const poolCoverage = new Set();
  let tick = 0;
  let index = 0;
  let maxActive = 0;
  let maxWaiting = 0;
  let blockedDisabled = 0;
  let blockedUnknownTool = 0;
  let blockedReviewGate = 0;
  let blockedConflict = 0;
  let started = 0;

  const finish = () => {
    for (let i = active.length - 1; i >= 0; i -= 1) {
      const item = active[i];
      if (item.end > tick) continue;
      for (const lock of item.op.locks) {
        const owner = lockOwners.get(lock);
        if (owner !== item.op.id) violations.push(`lock owner mismatch at finish tick ${tick}: ${lock} owner=${owner} finishing=${item.op.id}`);
        lockOwners.delete(lock);
      }
      workerCounts.set(item.op.workerKey, Math.max(0, (workerCounts.get(item.op.workerKey) || 0) - 1));
      events.push({ event: 'finish', tick, op: item.op.id, serverId: item.op.serverId });
      active.splice(i, 1);
    }
  };

  const tryStart = (operation) => {
    profileCoverage.add(operation.profile.id);
    poolCoverage.add(operation.profile.pool);
    if (!operation.enabled) { blockedDisabled += 1; events.push({ event: 'blocked-disabled', tick, op: operation.id, serverId: operation.serverId }); return true; }
    if (!operation.knownTool) { blockedUnknownTool += 1; events.push({ event: 'blocked-unknown-tool', tick, op: operation.id, serverId: operation.serverId }); return true; }
    if ((operation.profile.safety.startsWith('P0_') || operation.profile.safety.startsWith('PX_') || operation.profile.reviewGate) && !operation.reviewApproved) {
      blockedReviewGate += 1;
      events.push({ event: 'blocked-review-gate', tick, op: operation.id, serverId: operation.serverId, safety: operation.profile.safety });
      return true;
    }
    const conflict = operation.locks.find((lock) => lockOwners.has(lock));
    if (conflict) { blockedConflict += 1; return false; }
    const count = workerCounts.get(operation.workerKey) || 0;
    if (count >= operation.profile.maxInFlight) { blockedConflict += 1; return false; }
    for (const lock of operation.locks) lockOwners.set(lock, operation.id);
    workerCounts.set(operation.workerKey, count + 1);
    active.push({ op: operation, end: tick + operation.dur });
    maxActive = Math.max(maxActive, active.length);
    started += 1;
    events.push({ event: 'start', tick, op: operation.id, serverId: operation.serverId, locks: operation.locks, workerKey: operation.workerKey });
    return true;
  };

  while (index < all.length || waiting.length || active.length) {
    finish();
    while (index < all.length && all[index].ready <= tick) waiting.push(all[index++]);
    maxWaiting = Math.max(maxWaiting, waiting.length);
    for (let i = 0; i < waiting.length; i += 1) {
      if (tryStart(waiting[i])) {
        waiting.splice(i, 1);
        i -= 1;
      }
    }
    assertActiveInvariants(active, tick, violations);
    tick += 1;
    if (tick > Math.max(20_000, iterations * 4)) {
      violations.push('simulation did not drain');
      break;
    }
  }

  if (lockOwners.size !== 0) violations.push(`lockOwners not empty after drain: ${lockOwners.size}`);
  if ([...workerCounts.values()].some((value) => value !== 0)) violations.push('workerCounts not empty after drain');

  return {
    seed,
    operations: iterations,
    ticks: tick,
    elapsedMs: round(msSince(startedNs)),
    opsPerMs: round(iterations / Math.max(0.001, msSince(startedNs))),
    started,
    blockedDisabled,
    blockedUnknownTool,
    blockedReviewGate,
    blockedConflict,
    maxActive,
    maxWaiting,
    profileCoverage: [...profileCoverage].sort(),
    poolCoverage: [...poolCoverage].sort(),
    violations,
    sample: events.slice(0, 200),
  };
}

function read(relativePath) { return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8'); }
function check(id, ok, detail) { return { id, ok: Boolean(ok), status: ok ? 'pass' : 'fail', detail }; }

function sourceChecks() {
  const lease = read('src/upstream/lease_runtime.rs');
  const probe = read('scripts/live-random-mcp-probe.mjs');
  const worker = read('scripts/adaptive-worker-plan.mjs');
  const config = read('mcpace.config.json');
  return [
    check('known-tool-gate', /validate_upstream_tool_known/.test(lease) && /current tools\/list/.test(lease), 'Brokered calls validate requested tool against current tools/list.'),
    check('unknown-tool-explicit-override', /MCPACE_ALLOW_UNKNOWN_UPSTREAM_TOOLS/.test(lease) && /allowUnknownTool/.test(lease), 'Unknown upstream tools require explicit override.'),
    check('safe-probe-no-tool-call', /method: 'tools\/list'/.test(probe) && !/method: 'tools\/call'/.test(probe), 'Live probe initializes and lists tools only; it never calls tools.'),
    check('server-side-requests-not-fulfilled', /respondToServerRequest/.test(probe) && /probe client does not implement/.test(probe), 'Probe rejects unexpected server-side requests.'),
    check('remote-session-affinity', /remote-http-session-pool/.test(worker) && /transportSessionId/.test(worker), 'Remote HTTP session pool carries transportSessionId affinity.'),
    check('credential-affinity', /credential-session-pool/.test(worker) && /credentialProfile/.test(worker), 'Credentialed pools carry credentialProfile affinity.'),
    check('no-default-upstreams', /"servers"\s*:\s*\{\}/.test(config), 'Source snapshot has no enabled upstream MCP servers by default.'),
  ];
}

function collect(args) {
  const started = nowNs();
  const seedBase = 0xace55;
  const runs = Array.from({ length: args.seeds }, (_, index) => simulate(args.iterations, seedBase + index * 8191));
  const summary = runs.reduce((acc, run) => {
    acc.operations += run.operations;
    acc.started += run.started;
    acc.blockedDisabled += run.blockedDisabled;
    acc.blockedUnknownTool += run.blockedUnknownTool;
    acc.blockedReviewGate += run.blockedReviewGate;
    acc.blockedConflict += run.blockedConflict;
    acc.maxActive = Math.max(acc.maxActive, run.maxActive);
    acc.maxWaiting = Math.max(acc.maxWaiting, run.maxWaiting);
    for (const profile of run.profileCoverage) acc.profileCoverage.add(profile);
    for (const pool of run.poolCoverage) acc.poolCoverage.add(pool);
    acc.violations.push(...run.violations.map((violation) => `seed ${run.seed}: ${violation}`));
    return acc;
  }, { operations: 0, started: 0, blockedDisabled: 0, blockedUnknownTool: 0, blockedReviewGate: 0, blockedConflict: 0, maxActive: 0, maxWaiting: 0, profileCoverage: new Set(), poolCoverage: new Set(), violations: [] });
  const elapsedMs = msSince(started);
  const simChecks = [
    check('simulation-drains-without-races', summary.violations.length === 0, 'Scheduler fuzz simulation drains without overlapping lock/max-in-flight races.'),
    check('all-profile-kinds-covered', summary.profileCoverage.size === PROFILES.length, `covered ${summary.profileCoverage.size}/${PROFILES.length} profiles`),
    check('disabled-blocks', summary.blockedDisabled > 0, 'Disabled servers block before scheduling.'),
    check('unknown-tool-blocks', summary.blockedUnknownTool > 0, 'Unknown tools block before forwarding.'),
    check('review-gate-blocks', summary.blockedReviewGate > 0, 'Unknown/high-risk profiles block behind review gate.'),
    check('safe-work-starts', summary.started > 0, 'Safe enabled operations still run.'),
    check('race-audit-overhead-bounded', elapsedMs < 3000, `${summary.operations} operations across ${args.seeds} seeds in ${round(elapsedMs)}ms`),
    ...sourceChecks(),
  ];
  const blockers = simChecks.filter((item) => !item.ok).map((item) => `${item.id}: ${item.detail}`);
  return {
    schema: 'mcpace.mcpRaceConditionAudit.v1',
    generatedAt: new Date().toISOString(),
    project: { name: deriveProjectName(), version: deriveProjectVersion() },
    status: blockers.length ? 'fail' : 'pass',
    iterations: args.iterations,
    seeds: args.seeds,
    summary: {
      checks: simChecks.length,
      blockers: blockers.length,
      operations: summary.operations,
      started: summary.started,
      blockedDisabled: summary.blockedDisabled,
      blockedUnknownTool: summary.blockedUnknownTool,
      blockedReviewGate: summary.blockedReviewGate,
      blockedConflict: summary.blockedConflict,
      maxActive: summary.maxActive,
      maxWaiting: summary.maxWaiting,
      elapsedMs: round(elapsedMs),
      opsPerMs: round(summary.operations / Math.max(0.001, elapsedMs)),
      profileCoverage: [...summary.profileCoverage].sort(),
      poolCoverage: [...summary.poolCoverage].sort(),
      violations: summary.violations,
    },
    simulation: {
      runs,
      violations: summary.violations,
    },
    checks: simChecks,
    blockers,
  };
}

function renderMarkdown(report) {
  const lines = [
    '# MCP race-condition audit',
    '',
    `Generated: ${report.generatedAt}`,
    `Status: **${report.status}**`,
    '',
    `Operations: ${report.summary.operations}; seeds: ${report.seeds}; started: ${report.summary.started}; disabled blocks: ${report.summary.blockedDisabled}; unknown-tool blocks: ${report.summary.blockedUnknownTool}; review-gate blocks: ${report.summary.blockedReviewGate}; conflicts delayed: ${report.summary.blockedConflict}.`,
    `Overhead: ${report.summary.elapsedMs} ms (${report.summary.opsPerMs} ops/ms).`,
    '',
    '## Checks',
    '',
  ];
  for (const item of report.checks) lines.push(`- ${item.ok ? 'PASS' : 'FAIL'} ${item.id}: ${item.detail}`);
  return `${lines.join('\n')}\n`;
}

function writeReport(report, args) {
  if (args.write) {
    fs.mkdirSync(path.dirname(args.write), { recursive: true });
    fs.writeFileSync(args.write, JSON.stringify(report, null, 2) + '\n');
  }
  if (args.markdown) {
    fs.mkdirSync(path.dirname(args.markdown), { recursive: true });
    fs.writeFileSync(args.markdown, renderMarkdown(report));
  }
}

function main() {
  try {
    const args = parseArgs(process.argv.slice(2));
    if (args.help) { help(); return; }
    const report = collect(args);
    writeReport(report, args);
    if (args.json) console.log(JSON.stringify(report, null, 2));
    if (args.strict && report.status !== 'pass') process.exitCode = 1;
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}

main();
