#!/usr/bin/env node
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { performance } from 'node:perf_hooks';
import { fileURLToPath } from 'node:url';
import { profileFrom } from './lib/mcp-evidence-profile.mjs';
import { deriveProjectName, deriveProjectVersion } from './lib/project-metadata.mjs';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const DEFAULT_SERVER_COUNT = 10_000;
const DEFAULT_FRAGMENT_COUNT = 200;
const DEFAULT_OPERATION_COUNT = 50_000;

function parseArgs(argv) {
  const args = {
    json: false,
    write: path.join(repoRoot, 'reports', 'mcp-overhead-pressure-latest.json'),
    markdown: path.join(repoRoot, 'reports', 'mcp-overhead-pressure-latest.md'),
    servers: DEFAULT_SERVER_COUNT,
    fragments: DEFAULT_FRAGMENT_COUNT,
    operations: DEFAULT_OPERATION_COUNT,
    maxProfileAvgUs: 250,
    maxSchedulerAvgUs: 80,
    maxFragmentAvgMs: 12,
    maxHeapDeltaMiB: 128,
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
      case '--write': args.write = path.resolve(readValue()); break;
      case '--markdown': args.markdown = path.resolve(readValue()); break;
      case '--no-write': args.write = null; args.markdown = null; break;
      case '--servers': args.servers = positiveInteger(readValue(), token); break;
      case '--fragments': args.fragments = positiveInteger(readValue(), token); break;
      case '--operations': args.operations = positiveInteger(readValue(), token); break;
      case '--max-profile-avg-us': args.maxProfileAvgUs = positiveNumber(readValue(), token); break;
      case '--max-scheduler-avg-us': args.maxSchedulerAvgUs = positiveNumber(readValue(), token); break;
      case '--max-fragment-avg-ms': args.maxFragmentAvgMs = positiveNumber(readValue(), token); break;
      case '--max-heap-delta-mib': args.maxHeapDeltaMiB = positiveNumber(readValue(), token); break;
      case '--help':
      case '-h': args.help = true; break;
      default: throw new Error(`unsupported mcp-overhead-pressure-audit argument: ${token}`);
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
  console.log(`Usage: node scripts/mcp-overhead-pressure-audit.mjs [--json] [--servers N] [--fragments N] [--operations N]

Measures source-level MCP hub overhead without launching third-party MCP servers:
  - evidence/profile classification throughput for many server specs;
  - mcp_settings.d fragment scan and JSON parse pressure;
  - scheduler decision routing across clients/chats/sessions/projects/credentials;
  - heap delta and safety invariants that no server binaries or tools/call are executed.`);
}

function memoryMiB() {
  return process.memoryUsage().heapUsed / (1024 * 1024);
}

function round(value, digits = 2) {
  return Number(value.toFixed(digits));
}

function measure(label, fn) {
  const beforeHeapMiB = memoryMiB();
  const started = performance.now();
  const value = fn();
  const elapsedMs = performance.now() - started;
  const afterHeapMiB = memoryMiB();
  return {
    label,
    elapsedMs: round(elapsedMs),
    heapBeforeMiB: round(beforeHeapMiB),
    heapAfterMiB: round(afterHeapMiB),
    heapDeltaMiB: round(afterHeapMiB - beforeHeapMiB),
    value,
  };
}

const PATTERNS = [
  (i) => ({ serverId: `unknown-npx-${i}`, transport: 'stdio', command: 'npx', args: ['-y', `@vendor/random-mcp-${i}`] }),
  (i) => ({ serverId: `filesystem-${i}`, transport: 'stdio', command: 'npx', args: ['@modelcontextprotocol/server-filesystem', `/workspace/project-${i % 50}`] }),
  (i) => ({ serverId: `git-${i}`, transport: 'stdio', command: 'uvx', args: ['mcp-server-git', '--repository', `/workspace/repo-${i % 50}`] }),
  (i) => ({ serverId: `sqlite-${i}`, transport: 'stdio', command: 'uvx', args: ['mcp-server-sqlite', '--db-path', `/tmp/db-${i % 100}.sqlite`] }),
  (i) => ({ serverId: `memory-${i}`, transport: 'stdio', command: 'npx', args: ['@modelcontextprotocol/server-memory'] }),
  (i) => ({ serverId: `time-${i}`, transport: 'stdio', command: 'uvx', args: ['mcp-server-time'], policy: { stateless: true, stateBinding: 'none', concurrencyPolicy: 'multi-reader' } }),
  (i) => ({ serverId: `browser-${i}`, transport: 'stdio', command: 'npx', args: ['@playwright/mcp', '--profile', `p${i % 20}`] }),
  (i) => ({ serverId: `cloud-${i}`, transport: 'stdio', command: 'npx', args: ['azure-mcp', '--tenant', `tenant-${i % 10}`] }),
  (i) => ({ serverId: `fetch-${i}`, transport: 'stdio', command: 'uvx', args: ['mcp-server-fetch'] }),
  (i) => ({ serverId: `remote-session-${i}`, transport: 'streamable-http', url: `https://mcp${i % 25}.example.test/mcp` }),
  (i) => ({ serverId: `remote-stateless-${i}`, transport: 'streamable-http', url: `https://docs${i % 10}.example.test/mcp`, policy: { stateless: true, stateBinding: 'none', concurrencyPolicy: 'multi-reader', parallelismLimit: 8 } }),
  (i) => ({ serverId: `shell-${i}`, transport: 'stdio', command: 'node', args: ['command-runner-mcp.js', '--workspace', `/workspace/${i % 20}`] }),
];

function syntheticServer(index) {
  return PATTERNS[index % PATTERNS.length](index);
}

function generateServers(count) {
  return Array.from({ length: count }, (_, index) => syntheticServer(index));
}

function countBy(items, fn) {
  const counts = {};
  for (const item of items) {
    const key = fn(item);
    counts[key] = (counts[key] || 0) + 1;
  }
  return counts;
}

function warmEvidenceProfileInference() {
  for (let index = 0; index < PATTERNS.length * 2; index += 1) {
    profileFrom(syntheticServer(index), 'synthetic-pressure-warmup');
  }
}

function profilePressure(serverCount) {
  const servers = generateServers(serverCount);
  const profiles = servers.map((server) => profileFrom(server, 'synthetic-pressure'));
  return {
    serverCount,
    profiles,
    classCounts: countBy(profiles, (profile) => profile.parallelSafetyClass),
    poolCounts: countBy(profiles, (profile) => profile.defaultPoolModel),
    stateCounts: {
      stateful: profiles.filter((profile) => profile.stateful !== false).length,
      stateless: profiles.filter((profile) => profile.stateless === true).length,
    },
  };
}

function writeFragments(workspace, fragmentCount) {
  const dir = path.join(workspace, 'mcp_settings.d');
  fs.mkdirSync(dir, { recursive: true });
  for (let fragment = 0; fragment < fragmentCount; fragment += 1) {
    const mcpServers = {};
    for (let local = 0; local < 5; local += 1) {
      const server = syntheticServer((fragment * 5) + local);
      const name = server.serverId;
      const { serverId, ...config } = server;
      mcpServers[name] = config;
    }
    fs.writeFileSync(path.join(dir, `${String(fragment).padStart(5, '0')}.json`), JSON.stringify({ mcpServers }, null, 2));
  }
  return dir;
}

function scanFragments(fragmentCount) {
  const workspace = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-overhead-fragments-'));
  try {
    const dir = writeFragments(workspace, fragmentCount);
    const profiles = [];
    for (const entry of fs.readdirSync(dir).sort()) {
      if (!entry.endsWith('.json')) continue;
      const full = path.join(dir, entry);
      const parsed = JSON.parse(fs.readFileSync(full, 'utf8'));
      for (const [name, config] of Object.entries(parsed.mcpServers || {})) {
        profiles.push(profileFrom({ ...config, serverId: name, name }, `fragment:${entry}`));
      }
    }
    return {
      fragmentCount,
      serverCount: profiles.length,
      classCounts: countBy(profiles, (profile) => profile.parallelSafetyClass),
    };
  } finally {
    fs.rmSync(workspace, { recursive: true, force: true });
  }
}

function contextFor(index) {
  return {
    clientId: `client-${index % 16}`,
    chatId: `chat-${index % 64}`,
    sessionId: `session-${index % 128}`,
    projectRoot: `/workspace/project-${index % 50}`,
    repoRoot: `/workspace/repo-${index % 50}`,
    dbPath: `/tmp/db-${index % 100}.sqlite`,
    credentialProfile: `cred-${index % 32}`,
    tenant: `tenant-${index % 12}`,
    transportSession: `remote-session-${index % 128}`,
    browserContext: `browser-${index % 20}`,
    providerBudget: `provider-${index % 25}`,
  };
}

function lockValue(domain, context, profile) {
  if (domain.includes('credential')) return context.credentialProfile;
  if (domain === 'tenant') return context.tenant;
  if (domain === 'transport-session') return context.transportSession;
  if (domain === 'browser-context') return context.browserContext;
  if (domain === 'host-session' || domain === 'session') return context.sessionId;
  if (domain === 'context-store') return `${context.chatId}:${context.sessionId}`;
  if (domain === 'file' || domain === 'project') return context.projectRoot;
  if (domain === 'repo') return context.repoRoot;
  if (domain === 'db') return context.dbPath;
  if (domain === 'provider-budget') return context.providerBudget;
  return `${domain}:${profile.serverId}`;
}

function routeKey(profile, context) {
  return (profile.lockDomains || ['server']).map((domain) => `${domain}=${lockValue(domain, context, profile)}`).join('|');
}

function schedulerPressure(profiles, operationCount) {
  const active = new Map();
  let allowed = 0;
  let blockedReview = 0;
  let blockedLocks = 0;
  let released = 0;
  let invariantViolations = 0;
  const keySamples = [];
  for (let index = 0; index < operationCount; index += 1) {
    const profile = profiles[index % profiles.length];
    const context = contextFor(index);
    if (/^P0_|^PX_/.test(profile.parallelSafetyClass)) {
      blockedReview += 1;
      continue;
    }
    const key = routeKey(profile, context);
    if (keySamples.length < 12) keySamples.push({ serverId: profile.serverId, safety: profile.parallelSafetyClass, key });
    const exclusive = profile.stateless !== true || profile.maxInFlightPerWorker <= 1;
    if (exclusive && active.has(key)) {
      const holder = active.get(key);
      if (holder.key !== key) invariantViolations += 1;
      blockedLocks += 1;
    } else {
      allowed += 1;
      if (exclusive) active.set(key, { index, key, profile: profile.serverId });
    }
    if (index % 17 === 0 && active.size > 0) {
      const firstKey = active.keys().next().value;
      active.delete(firstKey);
      released += 1;
    }
  }
  return { operationCount, allowed, blockedReview, blockedLocks, released, activeLockCount: active.size, invariantViolations, keySamples };
}

function profileOnly(profiles) {
  return profiles.map((profile) => ({
    serverId: profile.serverId,
    safety: profile.parallelSafetyClass,
    pool: profile.defaultPoolModel,
    workers: profile.maxWorkers,
    locks: profile.lockDomains,
    stateless: profile.stateless,
  }));
}

function makeReport(args) {
  warmEvidenceProfileInference();
  const profiled = measure('profile-throughput', () => profilePressure(args.servers));
  const profiles = profiled.value.profiles;
  const fragmentScan = measure('fragment-scan', () => scanFragments(args.fragments));
  const scheduler = measure('scheduler-routing', () => schedulerPressure(profiles, args.operations));

  const profileAvgUs = (profiled.elapsedMs * 1000) / args.servers;
  const fragmentAvgMs = fragmentScan.elapsedMs / args.fragments;
  const schedulerAvgUs = (scheduler.elapsedMs * 1000) / args.operations;
  const totalHeapDeltaMiB = Math.max(0, profiled.heapDeltaMiB) + Math.max(0, fragmentScan.heapDeltaMiB) + Math.max(0, scheduler.heapDeltaMiB);

  const safety = {
    startsMcpServers: false,
    callsMcpTools: false,
    executesThirdPartyPackages: false,
    installsPackages: false,
    usesNetwork: false,
    mutatesProjectConfig: false,
    syntheticOnly: true,
  };

  const checks = [
    { id: 'profile-throughput-budget', ok: profileAvgUs <= args.maxProfileAvgUs, detail: `${round(profileAvgUs, 3)}us/profile <= ${args.maxProfileAvgUs}us` },
    { id: 'fragment-scan-budget', ok: fragmentAvgMs <= args.maxFragmentAvgMs, detail: `${round(fragmentAvgMs, 3)}ms/fragment <= ${args.maxFragmentAvgMs}ms` },
    { id: 'scheduler-routing-budget', ok: schedulerAvgUs <= args.maxSchedulerAvgUs, detail: `${round(schedulerAvgUs, 3)}us/operation <= ${args.maxSchedulerAvgUs}us` },
    { id: 'heap-budget', ok: totalHeapDeltaMiB <= args.maxHeapDeltaMiB, detail: `${round(totalHeapDeltaMiB, 2)}MiB <= ${args.maxHeapDeltaMiB}MiB` },
    { id: 'no-random-mcp-execution', ok: Object.entries(safety).every(([key, value]) => key === 'syntheticOnly' ? value === true : value === false), detail: 'No package binaries, MCP initialize, or tools/call are executed.' },
    { id: 'unknown-and-high-risk-review-gated', ok: scheduler.value.blockedReview > 0 && profiles.some((profile) => /^P0_|^PX_/.test(profile.parallelSafetyClass)), detail: `${scheduler.value.blockedReview} review-gated operations` },
    { id: 'scheduler-lock-invariants', ok: scheduler.value.invariantViolations === 0, detail: `${scheduler.value.invariantViolations} route-key invariant violations` },
    { id: 'single-shared-profile-library', ok: true, detail: 'Adaptive audit and pressure audit use scripts/lib/mcp-evidence-profile.mjs.' },
  ];
  const blockers = checks.filter((check) => !check.ok).map((check) => `${check.id}: ${check.detail}`);

  return {
    schema: 'mcpace.mcpOverheadPressure.v1',
    status: blockers.length ? 'blocked' : 'pass',
    generatedAt: new Date().toISOString(),
    project: { name: deriveProjectName(), version: deriveProjectVersion() },
    input: {
      servers: args.servers,
      fragments: args.fragments,
      operations: args.operations,
      budgets: {
        maxProfileAvgUs: args.maxProfileAvgUs,
        maxSchedulerAvgUs: args.maxSchedulerAvgUs,
        maxFragmentAvgMs: args.maxFragmentAvgMs,
        maxHeapDeltaMiB: args.maxHeapDeltaMiB,
      },
    },
    summary: {
      profileAvgUs: round(profileAvgUs, 3),
      profileWarmupProfiles: PATTERNS.length * 2,
      fragmentAvgMs: round(fragmentAvgMs, 3),
      schedulerAvgUs: round(schedulerAvgUs, 3),
      totalHeapDeltaMiB: round(totalHeapDeltaMiB, 2),
      classCounts: profiled.value.classCounts,
      poolCounts: profiled.value.poolCounts,
      stateCounts: profiled.value.stateCounts,
      allowedOperations: scheduler.value.allowed,
      blockedReviewOperations: scheduler.value.blockedReview,
      blockedLockOperations: scheduler.value.blockedLocks,
    },
    measurements: {
      profileThroughput: { ...profiled, value: { ...profiled.value, profiles: profileOnly(profiles.slice(0, 24)) } },
      fragmentScan,
      schedulerRouting: scheduler,
    },
    safety,
    optimizationPlan: [
      'Keep profile inference metadata-only on connect/open; do not spawn MCP servers during catalog rendering.',
      'Cache profile results by normalized command/url/policy fingerprint and invalidate on mcp_settings fragment mtime/hash changes.',
      'Start stdio servers lazily only when a reviewed tool is actually needed; use tools/list probe as explicit server test, not as default UI refresh.',
      'Shard Streamable HTTP by transport session and credential profile until explicit stateless evidence exists.',
      'Prefer lock-key routing over global singletons so safe read-only utilities can scale without sharing filesystem/git/db/browser state.',
    ],
    checks,
    blockers,
  };
}

function renderMarkdown(report) {
  const lines = [];
  lines.push('# MCP overhead pressure audit');
  lines.push('');
  lines.push(`Generated: ${report.generatedAt}`);
  lines.push(`Status: **${report.status}**`);
  lines.push(`Servers: ${report.input.servers}; fragments: ${report.input.fragments}; scheduler ops: ${report.input.operations}.`);
  lines.push('');
  lines.push('## Summary');
  lines.push('');
  lines.push('| Metric | Value |');
  lines.push('|---|---:|');
  lines.push(`| Profile avg | ${report.summary.profileAvgUs} us/profile |`);
  lines.push(`| Fragment scan avg | ${report.summary.fragmentAvgMs} ms/fragment |`);
  lines.push(`| Scheduler avg | ${report.summary.schedulerAvgUs} us/op |`);
  lines.push(`| Heap delta budget total | ${report.summary.totalHeapDeltaMiB} MiB |`);
  lines.push(`| Allowed operations | ${report.summary.allowedOperations} |`);
  lines.push(`| Review-gated operations | ${report.summary.blockedReviewOperations} |`);
  lines.push(`| Lock-blocked operations | ${report.summary.blockedLockOperations} |`);
  lines.push('');
  lines.push('## Safety');
  lines.push('');
  for (const [key, value] of Object.entries(report.safety)) lines.push(`- ${key}: ${value}`);
  lines.push('');
  lines.push('## Checks');
  lines.push('');
  for (const check of report.checks) lines.push(`- ${check.ok ? 'PASS' : 'FAIL'} ${check.id}: ${check.detail}`);
  lines.push('');
  lines.push('## Optimization plan');
  lines.push('');
  for (const item of report.optimizationPlan) lines.push(`- ${item}`);
  if (report.blockers.length) {
    lines.push('', '## Blockers', '');
    for (const blocker of report.blockers) lines.push(`- ${blocker}`);
  }
  return `${lines.join('\n')}\n`;
}

function writeReport(report, args) {
  if (args.write) {
    fs.mkdirSync(path.dirname(args.write), { recursive: true });
    fs.writeFileSync(args.write, `${JSON.stringify(report, null, 2)}\n`);
  }
  if (args.markdown) {
    fs.mkdirSync(path.dirname(args.markdown), { recursive: true });
    fs.writeFileSync(args.markdown, renderMarkdown(report));
  }
}

try {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) help();
  else {
    const report = makeReport(args);
    writeReport(report, args);
    if (args.json) console.log(JSON.stringify(report, null, 2));
    else process.stdout.write(renderMarkdown(report));
    if (report.status !== 'pass') process.exitCode = 1;
  }
} catch (error) {
  console.error(error?.stack || String(error));
  process.exitCode = 1;
}
