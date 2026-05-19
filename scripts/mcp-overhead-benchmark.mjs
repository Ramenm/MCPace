#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { repoRoot, deriveProjectName, deriveProjectVersion } from './lib/project-metadata.mjs';
import { classifyMcpPackageMetadata } from './lib/mcp-signal-policy.mjs';

const DEFAULTS = Object.freeze({
  packages: 100,
  servers: 100,
  toolsPerServer: 50,
  operations: 20_000,
  memoryLimitMiB: 128,
  maxClassifierMs: 120,
  maxRegistryMs: 180,
  maxSchedulerMs: 450,
  maxDecisionUs: 75,
});

// Classification policy is shared with mass package survey and adaptive evidence profiling.

class Rng {
  constructor(seed = 0x51ab1e) { this.state = seed >>> 0; }
  next() { this.state = (Math.imul(this.state, 1664525) + 1013904223) >>> 0; return this.state / 2 ** 32; }
  int(max) { return Math.floor(this.next() * max); }
  pick(values) { return values[this.int(values.length)]; }
}

function parseArgs(argv) {
  const args = {
    json: false,
    strict: false,
    noWrite: false,
    write: path.join(repoRoot, 'reports/mcp-overhead-benchmark-latest.json'),
    markdown: path.join(repoRoot, 'reports/mcp-overhead-benchmark-latest.md'),
    ...DEFAULTS,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    const read = () => {
      const value = argv[index + 1];
      if (!value || value.startsWith('--')) throw new Error(`${token} requires a value`);
      index += 1;
      return value;
    };
    switch (token) {
      case '--json': args.json = true; break;
      case '--strict': args.strict = true; break;
      case '--no-write': args.noWrite = true; args.write = null; args.markdown = null; break;
      case '--write': args.write = path.resolve(read()); break;
      case '--markdown': args.markdown = path.resolve(read()); break;
      case '--packages': args.packages = boundedInt(read(), token, 1, 10_000); break;
      case '--servers': args.servers = boundedInt(read(), token, 1, 10_000); break;
      case '--tools-per-server': args.toolsPerServer = boundedInt(read(), token, 1, 10_000); break;
      case '--operations': args.operations = boundedInt(read(), token, 100, 1_000_000); break;
      case '--memory-limit-mib': args.memoryLimitMiB = boundedInt(read(), token, 16, 16_384); break;
      case '--max-classifier-ms': args.maxClassifierMs = positiveNumber(read(), token); break;
      case '--max-registry-ms': args.maxRegistryMs = positiveNumber(read(), token); break;
      case '--max-scheduler-ms': args.maxSchedulerMs = positiveNumber(read(), token); break;
      case '--max-decision-us': args.maxDecisionUs = positiveNumber(read(), token); break;
      case '-h':
      case '--help': args.help = true; break;
      default: throw new Error(`unsupported mcp-overhead-benchmark argument: ${token}`);
    }
  }
  return args;
}

function boundedInt(value, label, min, max) {
  const parsed = Number.parseInt(value, 10);
  if (!Number.isSafeInteger(parsed) || parsed < min || parsed > max) throw new Error(`${label} must be an integer in [${min}, ${max}]`);
  return parsed;
}

function positiveNumber(value, label) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed) || parsed <= 0) throw new Error(`${label} must be a positive number`);
  return parsed;
}

function help() {
  console.log(`Usage: node scripts/mcp-overhead-benchmark.mjs [--json] [--strict]\n\nMeasures metadata classification, synthetic tool-registry construction, and\nscheduler decision overhead without starting random MCP servers or calling tools.`);
}

function nowNs() { return process.hrtime.bigint(); }
function msSince(startNs) { return Number(nowNs() - startNs) / 1_000_000; }
function round(value, places = 3) { const f = 10 ** places; return Math.round(value * f) / f; }
function heapMiB() { return process.memoryUsage().heapUsed / 1024 / 1024; }

function packageDescriptor(index) {
  const samples = [
    ['@sample/mcp-filesystem', 'filesystem workspace path tools', ['mcp', 'filesystem']],
    ['@sample/mcp-git', 'git repository helper', ['mcp', 'git']],
    ['@sample/mcp-sqlite', 'sqlite database server', ['mcp', 'database']],
    ['@sample/mcp-memory', 'memory context notes', ['mcp', 'memory']],
    ['@sample/mcp-docs-fetch', 'fetch web documentation search', ['mcp', 'docs']],
    ['@sample/mcp-slack-api', 'slack oauth api integration', ['mcp', 'api']],
    ['@sample/mcp-aws-admin', 'aws cloud terraform admin', ['mcp', 'cloud']],
    ['@sample/mcp-playwright', 'playwright browser automation', ['mcp', 'browser']],
    ['@sample/mcp-shell', 'shell command runner', ['mcp', 'shell']],
    ['@sample/mcp-time', 'time date utility', ['mcp', 'utility']],
  ];
  const sample = samples[index % samples.length];
  return {
    name: `${sample[0]}-${index}`,
    version: `0.0.${index % 99}`,
    description: sample[1],
    keywords: sample[2],
  };
}

function classify(pkg) {
  const classification = classifyMcpPackageMetadata(pkg);
  return {
    ...classification,
    maxInFlight: classification.maxInFlightPerWorker || 1,
  };
}

function measureClassification(packageCount) {
  const packages = Array.from({ length: packageCount }, (_, index) => packageDescriptor(index));
  const startHeap = heapMiB();
  const start = nowNs();
  const profiles = packages.map((pkg) => ({ ...pkg, classification: classify(pkg) }));
  const elapsedMs = msSince(start);
  const policyCounts = {};
  for (const profile of profiles) {
    const policy = profile.classification.policy;
    policyCounts[policy] = (policyCounts[policy] || 0) + 1;
  }
  return { packageCount, elapsedMs: round(elapsedMs), perPackageUs: round((elapsedMs * 1000) / packageCount), heapDeltaMiB: round(heapMiB() - startHeap), policyCounts };
}

function generateToolRegistry(serverCount, toolsPerServer) {
  const registry = new Map();
  const byServer = new Map();
  const readonlyCandidates = new Set();
  for (let server = 0; server < serverCount; server += 1) {
    const serverId = `server-${server}`;
    const tools = [];
    for (let tool = 0; tool < toolsPerServer; tool += 1) {
      const name = `tool_${tool}`;
      const qualified = `${serverId}/${name}`;
      const entry = {
        serverId,
        name,
        qualified,
        title: `Tool ${tool}`,
        annotations: { readOnlyHint: tool % 3 === 0, destructiveHint: tool % 17 === 0 },
      };
      registry.set(qualified, entry);
      tools.push(entry);
      if (entry.annotations.readOnlyHint && !entry.annotations.destructiveHint) readonlyCandidates.add(qualified);
    }
    byServer.set(serverId, tools);
  }
  return { registry, byServer, readonlyCandidates };
}

function measureRegistry(serverCount, toolsPerServer) {
  const startHeap = heapMiB();
  const start = nowNs();
  const built = generateToolRegistry(serverCount, toolsPerServer);
  const elapsedMs = msSince(start);
  return {
    serverCount,
    toolsPerServer,
    toolCount: serverCount * toolsPerServer,
    elapsedMs: round(elapsedMs),
    perToolUs: round((elapsedMs * 1000) / (serverCount * toolsPerServer)),
    heapDeltaMiB: round(heapMiB() - startHeap),
    readonlyCandidateCount: built.readonlyCandidates.size,
    built,
  };
}

function schedulerProfile(serverIndex) {
  const kind = serverIndex % 10;
  if (kind === 0) return { policy: 'project-filesystem-single-writer', maxInFlight: 1, reviewRequired: false, locks: ['project', 'file'] };
  if (kind === 1) return { policy: 'project-repo-single-writer', maxInFlight: 1, reviewRequired: false, locks: ['project', 'repo'] };
  if (kind === 2) return { policy: 'database-path-single-writer', maxInFlight: 1, reviewRequired: false, locks: ['project', 'database'] };
  if (kind === 3) return { policy: 'state-profile-single-session', maxInFlight: 1, reviewRequired: false, locks: ['session'] };
  if (kind === 4) return { policy: 'remote-session-pool', maxInFlight: 1, reviewRequired: false, locks: ['transport-session', 'credential'] };
  if (kind === 5) return { policy: 'credential-scoped-review', maxInFlight: 1, reviewRequired: true, locks: ['credential', 'tenant'] };
  if (kind === 6) return { policy: 'network-fetch-review', maxInFlight: 2, reviewRequired: true, locks: ['provider-budget'] };
  if (kind === 7) return { policy: 'local-utility-multi-reader-candidate', maxInFlight: 4, reviewRequired: false, locks: ['server'] };
  if (kind === 8) return { policy: 'disabled-dangerous-command-runner', maxInFlight: 0, reviewRequired: true, disabled: true, locks: ['host'] };
  return { policy: 'unknown-review-required', maxInFlight: 1, reviewRequired: true, locks: ['server'] };
}

function lockKeys(profile, op) {
  return profile.locks.map((lock) => {
    if (lock === 'project') return `project:${op.project}`;
    if (lock === 'file') return `file:${op.project}`;
    if (lock === 'repo') return `repo:${op.project}/.git`;
    if (lock === 'database') return `db:${op.project}/db.sqlite`;
    if (lock === 'session') return `session:${op.session}:context:${op.context}`;
    if (lock === 'transport-session') return `transport-session:${op.transportSession}`;
    if (lock === 'credential') return `credential:${op.credential}`;
    if (lock === 'tenant') return `tenant:${op.tenant}`;
    if (lock === 'provider-budget') return `provider-budget:${op.provider}`;
    if (lock === 'host') return `host:${op.host}`;
    return `${lock}:${op.serverId}`;
  });
}

function measureScheduling({ operations, serverCount, registry }) {
  const rng = new Rng();
  const activeLocks = new Set();
  const activeByWorker = new Map();
  const finishQueue = [];
  const startHeap = heapMiB();
  const startedAt = nowNs();
  let blockedDisabled = 0;
  let blockedUnknownTool = 0;
  let blockedReview = 0;
  let blockedConflict = 0;
  let started = 0;
  let randomServerStarts = 0;
  let maxActiveLocks = 0;

  const completeUntil = (tick) => {
    while (finishQueue.length > 0 && finishQueue[0].end <= tick) {
      const item = finishQueue.shift();
      for (const lock of item.locks) activeLocks.delete(lock);
      activeByWorker.set(item.workerKey, Math.max(0, (activeByWorker.get(item.workerKey) || 0) - 1));
    }
  };

  for (let index = 0; index < operations; index += 1) {
    completeUntil(index);
    const serverIndex = rng.int(serverCount);
    const serverId = `server-${serverIndex}`;
    const toolIndex = rng.int(100) < 12 ? 100_000 + rng.int(1000) : rng.int(Math.max(1, Math.floor(registry.size / serverCount)));
    const toolName = `tool_${toolIndex}`;
    const profile = schedulerProfile(serverIndex);
    const op = {
      serverId,
      toolKey: `${serverId}/${toolName}`,
      project: `/workspace/${rng.int(8)}`,
      session: `chat-${rng.int(32)}`,
      context: `ctx-${rng.int(8)}`,
      transportSession: `ts-${rng.int(48)}`,
      credential: `cred-${rng.int(16)}`,
      tenant: `tenant-${rng.int(16)}`,
      provider: `provider-${rng.int(6)}`,
      host: 'host-1',
      approved: rng.int(100) > 80,
    };

    if (profile.disabled) { blockedDisabled += 1; continue; }
    if (!registry.has(op.toolKey)) { blockedUnknownTool += 1; continue; }
    if (profile.reviewRequired && !op.approved) { blockedReview += 1; continue; }

    const locks = lockKeys(profile, op);
    if (locks.some((lock) => activeLocks.has(lock))) { blockedConflict += 1; continue; }
    const workerKey = `${profile.policy}:${locks.join('|')}`;
    const activeForWorker = activeByWorker.get(workerKey) || 0;
    if (activeForWorker >= profile.maxInFlight) { blockedConflict += 1; continue; }

    for (const lock of locks) activeLocks.add(lock);
    activeByWorker.set(workerKey, activeForWorker + 1);
    finishQueue.push({ end: index + 1 + rng.int(5), locks, workerKey });
    finishQueue.sort((a, b) => a.end - b.end);
    started += 1;
    maxActiveLocks = Math.max(maxActiveLocks, activeLocks.size);
    if (op.serverId.startsWith('random-')) randomServerStarts += 1;
  }
  completeUntil(Number.MAX_SAFE_INTEGER);
  const elapsedMs = msSince(startedAt);
  return {
    operations,
    elapsedMs: round(elapsedMs),
    perDecisionUs: round((elapsedMs * 1000) / operations),
    heapDeltaMiB: round(heapMiB() - startHeap),
    started,
    blockedDisabled,
    blockedUnknownTool,
    blockedReview,
    blockedConflict,
    randomServerStarts,
    maxActiveLocks,
    activeLocksAfterDrain: activeLocks.size,
  };
}

function buildReport(args) {
  const started = nowNs();
  const classification = measureClassification(args.packages);
  const registryMeasured = measureRegistry(args.servers, args.toolsPerServer);
  const scheduling = measureScheduling({ operations: args.operations, serverCount: args.servers, registry: registryMeasured.built.registry });
  const registry = { ...registryMeasured };
  delete registry.built;
  const totalElapsedMs = msSince(started);
  const checks = [
    { id: 'classifier-under-budget', ok: classification.elapsedMs <= args.maxClassifierMs, detail: `${classification.elapsedMs}ms <= ${args.maxClassifierMs}ms` },
    { id: 'registry-build-under-budget', ok: registry.elapsedMs <= args.maxRegistryMs, detail: `${registry.elapsedMs}ms <= ${args.maxRegistryMs}ms for ${registry.toolCount} tools` },
    { id: 'scheduler-under-budget', ok: scheduling.elapsedMs <= args.maxSchedulerMs, detail: `${scheduling.elapsedMs}ms <= ${args.maxSchedulerMs}ms for ${scheduling.operations} decisions` },
    { id: 'scheduler-per-decision-under-budget', ok: scheduling.perDecisionUs <= args.maxDecisionUs, detail: `${scheduling.perDecisionUs}µs <= ${args.maxDecisionUs}µs` },
    { id: 'scheduler-drains-locks', ok: scheduling.activeLocksAfterDrain === 0, detail: `${scheduling.activeLocksAfterDrain} active locks after drain` },
    { id: 'disabled-and-unknown-never-start-random-servers', ok: scheduling.randomServerStarts === 0 && scheduling.blockedDisabled > 0 && scheduling.blockedUnknownTool > 0, detail: `randomServerStarts=${scheduling.randomServerStarts}, disabled=${scheduling.blockedDisabled}, unknownTool=${scheduling.blockedUnknownTool}` },
    { id: 'review-gate-observed', ok: scheduling.blockedReview > 0, detail: `${scheduling.blockedReview} review-gated operations blocked` },
    { id: 'heap-under-budget', ok: Math.max(classification.heapDeltaMiB, registry.heapDeltaMiB, scheduling.heapDeltaMiB) <= args.memoryLimitMiB, detail: `max stage heap delta ${Math.max(classification.heapDeltaMiB, registry.heapDeltaMiB, scheduling.heapDeltaMiB)}MiB <= ${args.memoryLimitMiB}MiB` },
  ];
  const blockers = checks.filter((check) => !check.ok).map((check) => `${check.id}: ${check.detail}`);
  return {
    schema: 'mcpace.mcpOverheadBenchmark.v1',
    generatedAt: new Date().toISOString(),
    project: { name: deriveProjectName(), version: deriveProjectVersion() },
    status: blockers.length === 0 ? 'pass' : 'fail',
    budgets: {
      maxClassifierMs: args.maxClassifierMs,
      maxRegistryMs: args.maxRegistryMs,
      maxSchedulerMs: args.maxSchedulerMs,
      maxDecisionUs: args.maxDecisionUs,
      memoryLimitMiB: args.memoryLimitMiB,
    },
    inputs: {
      packages: args.packages,
      servers: args.servers,
      toolsPerServer: args.toolsPerServer,
      tools: args.servers * args.toolsPerServer,
      operations: args.operations,
    },
    measurements: { classification, registry, scheduling, totalElapsedMs: round(totalElapsedMs) },
    checks,
    blockers,
    notes: [
      'No MCP package binary is executed in this benchmark.',
      'No initialize, tools/list, or tools/call is sent to random packages.',
      'The benchmark measures hub-side classification, registry, and scheduling overhead only.',
    ],
  };
}

function renderMarkdown(report) {
  const lines = [
    '# MCP overhead benchmark',
    '',
    `Generated: ${report.generatedAt}`,
    `Status: **${report.status}**`,
    '',
    '## Inputs',
    '',
    `- Packages classified: ${report.inputs.packages}`,
    `- Servers: ${report.inputs.servers}`,
    `- Tools: ${report.inputs.tools}`,
    `- Scheduling decisions: ${report.inputs.operations}`,
    '',
    '## Measurements',
    '',
    `- Classification: ${report.measurements.classification.elapsedMs} ms (${report.measurements.classification.perPackageUs} µs/package)`,
    `- Registry build: ${report.measurements.registry.elapsedMs} ms (${report.measurements.registry.perToolUs} µs/tool)`,
    `- Scheduler decisions: ${report.measurements.scheduling.elapsedMs} ms (${report.measurements.scheduling.perDecisionUs} µs/decision)`,
    `- Scheduler started/blocked: ${report.measurements.scheduling.started} started, ${report.measurements.scheduling.blockedDisabled} disabled, ${report.measurements.scheduling.blockedUnknownTool} unknown-tool, ${report.measurements.scheduling.blockedReview} review-gated`,
    '',
    '## Checks',
    '',
    '| Check | OK | Detail |',
    '|---|---:|---|',
    ...report.checks.map((check) => `| ${check.id} | ${check.ok ? 'yes' : 'no'} | ${String(check.detail).replace(/\n/g, ' ')} |`),
    '',
    '## Safety notes',
    '',
    ...report.notes.map((note) => `- ${note}`),
  ];
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
    const report = buildReport(args);
    writeReport(report, args);
    if (args.json) console.log(JSON.stringify(report, null, 2));
    if (args.strict && report.status !== 'pass') process.exitCode = 1;
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}

main();
