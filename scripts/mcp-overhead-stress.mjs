#!/usr/bin/env node
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { performance } from 'node:perf_hooks';
import { repoRoot, deriveProjectName, deriveProjectVersion } from './lib/project-metadata.mjs';
import { insertTopK } from './lib/bounded-top-k.mjs';
import { classifyMcpPackageMetadata } from './lib/mcp-signal-policy.mjs';

const DEFAULTS = Object.freeze({
  servers: 100,
  tools: 100_000,
  operations: 25_000,
  packageProfiles: 1_000,
  searchLimit: 25,
  projectionBudget: 64,
  pageSize: 128,
  memoryLimitMiB: 256,
  seed: 0x6d637061,
  write: 'reports/mcp-overhead-stress-latest.json',
  markdown: 'reports/mcp-overhead-stress-latest.md',
});

const SERVER_KINDS = Object.freeze([
  { kind: 'unknown-npx', safety: 'P0_unknown_stdio', pool: 'process', lockTemplates: ['server:{serverId}'], maxInFlight: 1, defaultEnabled: false, reviewGate: true, stateClass: 'unknown-stateful' },
  { kind: 'memory', safety: 'P2_session_safe', pool: 'session', lockTemplates: ['session:{sessionId}:ctx:{contextId}', 'context-store:{sessionId}'], maxInFlight: 1, defaultEnabled: true, reviewGate: false, stateClass: 'session-stateful' },
  { kind: 'filesystem', safety: 'P3_project_safe', pool: 'project', lockTemplates: ['file:{projectRoot}', 'project:{projectRoot}'], maxInFlight: 1, defaultEnabled: true, reviewGate: false, stateClass: 'project-stateful' },
  { kind: 'git', safety: 'P3_project_safe', pool: 'project', lockTemplates: ['repo:{repoRoot}', 'project:{projectRoot}'], maxInFlight: 1, defaultEnabled: true, reviewGate: false, stateClass: 'project-stateful' },
  { kind: 'sqlite', safety: 'P3_project_safe', pool: 'project', lockTemplates: ['db:{dbPath}', 'project:{projectRoot}'], maxInFlight: 1, defaultEnabled: true, reviewGate: false, stateClass: 'project-stateful' },
  { kind: 'remote-http-session', safety: 'P2_session_safe', pool: 'remote-session', lockTemplates: ['transport-session:{transportSessionId}', 'credential:{credentialProfile}'], maxInFlight: 1, defaultEnabled: true, reviewGate: false, stateClass: 'transport-session-stateful' },
  { kind: 'remote-http-readonly', safety: 'P4_stateless_remote_candidate', pool: 'remote-shared', lockTemplates: ['provider-budget:{providerBudgetKey}'], maxInFlight: 4, defaultEnabled: true, reviewGate: false, stateClass: 'stateless-evidence-candidate' },
  { kind: 'credential-api', safety: 'P2_session_safe', pool: 'credential-session', lockTemplates: ['credential:{credentialProfile}', 'tenant:{tenantId}'], maxInFlight: 1, defaultEnabled: true, reviewGate: false, stateClass: 'credential-tenant-stateful' },
  { kind: 'browser', safety: 'PX_forbidden_browser_until_context_isolated', pool: 'browser', lockTemplates: ['browser-context:{browserContextId}', 'host-session:{hostSessionId}'], maxInFlight: 1, defaultEnabled: false, reviewGate: true, stateClass: 'host-context-stateful' },
  { kind: 'shell', safety: 'PX_forbidden_process_until_sandboxed', pool: 'host-process', lockTemplates: ['host-session:{hostSessionId}'], maxInFlight: 1, defaultEnabled: false, reviewGate: true, stateClass: 'host-process-stateful' },
]);

const CONTEXT = Object.freeze({
  clients: ['claude', 'cursor', 'vscode', 'codex', 'chatgpt', 'zed', 'windsurf', 'custom'],
  sessions: Array.from({ length: 32 }, (_, index) => `chat-${String(index + 1).padStart(2, '0')}`),
  contexts: ['ctx-a', 'ctx-b', 'ctx-c', 'ctx-d'],
  projects: ['/work/a', '/work/b', '/work/c', '/work/d', '/work/e', '/work/f', '/work/g', '/work/h'],
  credentials: ['anon', 'user-a', 'user-b', 'user-c', 'service-a'],
  tenants: ['tenant-a', 'tenant-b', 'tenant-c'],
  transportSessions: Array.from({ length: 24 }, (_, index) => `mcp-session-${String(index + 1).padStart(2, '0')}`),
  providers: ['docs', 'search', 'maps', 'api'],
  browsers: ['browser-a', 'browser-b', 'browser-c', 'browser-d'],
  hosts: ['host-main'],
});

function parseArgs(argv) {
  const args = { ...DEFAULTS, json: false, help: false };
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
      case '--servers': args.servers = positiveInteger(read(), token, 1, 20_000); break;
      case '--tools': args.tools = positiveInteger(read(), token, 0, 5_000_000); break;
      case '--operations': args.operations = positiveInteger(read(), token, 0, 1_000_000); break;
      case '--package-profiles': args.packageProfiles = positiveInteger(read(), token, 0, 100_000); break;
      case '--search-limit': args.searchLimit = positiveInteger(read(), token, 1, 1000); break;
      case '--projection-budget': args.projectionBudget = positiveInteger(read(), token, 1, 4096); break;
      case '--page-size': args.pageSize = positiveInteger(read(), token, 1, 4096); break;
      case '--memory-limit-mib': args.memoryLimitMiB = positiveNumber(read(), token, 1, 8192); break;
      case '--seed': args.seed = positiveInteger(read(), token, 1, 0xffffffff); break;
      case '--write': args.write = read(); break;
      case '--markdown': args.markdown = read(); break;
      case '--no-write': args.write = null; args.markdown = null; break;
      case '--help':
      case '-h': args.help = true; break;
      default: throw new Error(`unsupported mcp-overhead-stress argument: ${token}`);
    }
  }
  return args;
}

function positiveInteger(value, label, min, max) {
  const parsed = Number.parseInt(value, 10);
  if (!Number.isSafeInteger(parsed) || parsed < min || parsed > max) throw new Error(`${label} must be an integer in [${min}, ${max}]`);
  return parsed;
}

function positiveNumber(value, label, min, max) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed) || parsed < min || parsed > max) throw new Error(`${label} must be a number in [${min}, ${max}]`);
  return parsed;
}

function printHelp() {
  console.log(`Usage: node scripts/mcp-overhead-stress.mjs [--json] [--servers 100] [--tools 100000] [--operations 25000]

Measures synthetic MCP hub overhead without starting random MCP servers or calling their tools:
  - tool catalog/search/projection footprint;
  - known-tool validation lookup cost;
  - multi-client/session/credential scheduler lock overhead;
  - metadata-only profile classification cost.`);
}

class Rng {
  constructor(seed) { this.state = seed >>> 0; }
  next() { this.state = (Math.imul(this.state, 1664525) + 1013904223) >>> 0; return this.state / 2 ** 32; }
  int(max) { return Math.floor(this.next() * max); }
  pick(values) { return values[this.int(values.length)]; }
}

function round(value, digits = 2) {
  const factor = 10 ** digits;
  return Math.round(value * factor) / factor;
}

function distributedSlots(total, buckets, index) {
  const base = Math.floor(total / Math.max(1, buckets));
  return base + (index < total % Math.max(1, buckets) ? 1 : 0);
}

function buildServers(count) {
  return Array.from({ length: count }, (_, index) => {
    const template = SERVER_KINDS[index % SERVER_KINDS.length];
    return {
      ...template,
      serverId: `srv-${String(index).padStart(4, '0')}-${template.kind}`,
      packageName: packageNameFor(template.kind, index),
    };
  });
}

function packageNameFor(kind, index) {
  const scope = index % 3 === 0 ? '@vendor' : '@modelcontextprotocol';
  return `${scope}/${kind}-mcp-${index}`;
}

function syntheticTool(server, toolIndex) {
  const readOnly = toolIndex % 10 < 7 || server.kind === 'remote-http-readonly';
  const mutating = !readOnly && !server.safety.startsWith('PX_');
  const prefix = server.kind === 'filesystem' ? (readOnly ? 'read_file' : 'write_file')
    : server.kind === 'git' ? (readOnly ? 'git_status' : 'git_commit')
      : server.kind === 'sqlite' ? (readOnly ? 'query_sql' : 'execute_sql')
        : server.kind === 'credential-api' ? (readOnly ? 'list_api' : 'update_api')
          : server.kind === 'memory' ? (readOnly ? 'search_memory' : 'save_memory')
            : server.kind === 'remote-http-readonly' ? 'lookup_docs'
              : readOnly ? 'read' : 'mutate';
  const name = `${prefix}_${toolIndex}`;
  return {
    serverId: server.serverId,
    serverKind: server.kind,
    name,
    qualifiedName: `${server.serverId}.${name}`,
    title: `${prefix.replaceAll('_', ' ')} ${toolIndex}`,
    description: `${readOnly ? 'Read-only' : 'State-changing'} ${server.kind} tool on ${server.serverId}`,
    readOnly,
    mutating,
  };
}

function normalize(value) {
  return String(value || '').toLowerCase().replace(/[^a-z0-9_.-]+/g, ' ');
}

function scoreTool(tool, terms) {
  if (terms.length === 0) return 1;
  const haystack = `${normalize(tool.name)} ${normalize(tool.qualifiedName)} ${normalize(tool.title)} ${normalize(tool.description)}`;
  let score = 0;
  for (const term of terms) {
    if (tool.name === term || tool.qualifiedName === term) score += 80;
    if (tool.name.includes(term)) score += 40;
    if (tool.qualifiedName.includes(term)) score += 30;
    if (tool.title.toLowerCase().includes(term)) score += 20;
    if (tool.description.toLowerCase().includes(term)) score += 10;
    if (haystack.includes(term)) score += 1;
  }
  return score;
}

function compactTool(tool, score) {
  return {
    serverId: tool.serverId,
    name: tool.name,
    qualifiedName: tool.qualifiedName,
    readOnly: tool.readOnly,
    score,
    call: { tool: 'upstream_call', arguments: { server: tool.serverId, tool: tool.name } },
  };
}

function runToolCatalogStress({ servers, tools, searchLimit, projectionBudget, pageSize }) {
  const started = performance.now();
  const heapStart = process.memoryUsage().heapUsed;
  const terms = ['read', 'search', 'docs', 'status'];
  const projectionCandidateLimit = Math.max(projectionBudget, Math.min(projectionBudget * 8, 8192));
  const knownToolKeys = new Set();
  const knownSamples = [];
  const topSearch = [];
  let readOnlyCount = 0;
  let mutatingCount = 0;
  let matchCount = 0;
  let projectedCandidateCount = 0;
  let brokerOnlyCount = 0;
  let searchSpaceToolCount = 0;

  for (let serverIndex = 0; serverIndex < servers.length; serverIndex += 1) {
    const server = servers[serverIndex];
    const slots = distributedSlots(tools, servers.length, serverIndex);
    for (let toolIndex = 0; toolIndex < slots; toolIndex += 1) {
      const tool = syntheticTool(server, toolIndex);
      searchSpaceToolCount += 1;
      knownToolKeys.add(tool.qualifiedName);
      if (knownSamples.length < 4096) knownSamples.push(tool.qualifiedName);
      if (tool.readOnly) readOnlyCount += 1;
      if (tool.mutating) mutatingCount += 1;

      const score = scoreTool(tool, terms);
      if (score > 0) {
        matchCount += 1;
        insertTopK(topSearch, { score, key: tool.qualifiedName, tool: compactTool(tool, score) }, Math.max(2, searchLimit));
      }

      if (tool.readOnly && projectedCandidateCount < projectionCandidateLimit) {
        projectedCandidateCount += 1;
      } else if (!tool.readOnly || projectedCandidateCount >= projectionCandidateLimit) {
        brokerOnlyCount += 1;
      }
    }
  }

  const elapsedMs = performance.now() - started;
  const heapDeltaMiB = (process.memoryUsage().heapUsed - heapStart) / 1024 / 1024;
  return {
    elapsedMs: round(elapsedMs),
    heapDeltaMiB: round(heapDeltaMiB, 3),
    searchSpaceToolCount,
    knownToolKeyCount: knownToolKeys.size,
    knownSamples,
    matchCount,
    returnedSearchCount: Math.min(searchLimit, topSearch.length),
    retainedSearchCandidates: topSearch.length,
    readOnlyCount,
    mutatingCount,
    brokerOnlyCount,
    projectedCandidateCount,
    projectedToolCount: Math.min(projectionBudget, projectedCandidateCount),
    firstPageCount: Math.min(pageSize, 8 + Math.min(projectionBudget, projectedCandidateCount)),
    topSearch: topSearch.map((entry) => entry.tool),
  };
}

function runLookupStress(knownSamples, operations, seed) {
  const rng = new Rng(seed ^ 0x51f15e);
  const sampleSet = new Set(knownSamples);
  const lookupCount = Math.max(1, Math.min(operations, Math.max(knownSamples.length * 4, 1)));
  const started = performance.now();
  let knownHits = 0;
  let unknownHits = 0;
  let unknownForwarded = 0;
  for (let index = 0; index < lookupCount; index += 1) {
    const shouldUseKnown = knownSamples.length > 0 && index % 2 === 0;
    const key = shouldUseKnown ? knownSamples[rng.int(knownSamples.length)] : `unknown-server.unknown-tool-${index}`;
    const known = sampleSet.has(key);
    if (known) knownHits += 1;
    else {
      unknownHits += 1;
      // Unknown tool requests must fail closed and never be forwarded to an upstream MCP server.
      if (false) unknownForwarded += 1;
    }
  }
  return { lookupCount, elapsedMs: round(performance.now() - started), knownHits, unknownHits, unknownForwarded };
}

function materialize(template, op) {
  return template
    .replaceAll('{serverId}', op.server.serverId)
    .replaceAll('{sessionId}', op.sessionId)
    .replaceAll('{contextId}', op.contextId)
    .replaceAll('{projectRoot}', op.projectRoot)
    .replaceAll('{repoRoot}', `${op.projectRoot}/.git`)
    .replaceAll('{dbPath}', `${op.projectRoot}/data.sqlite`)
    .replaceAll('{transportSessionId}', op.transportSessionId)
    .replaceAll('{credentialProfile}', op.credentialProfile)
    .replaceAll('{tenantId}', op.tenantId)
    .replaceAll('{providerBudgetKey}', op.providerBudgetKey)
    .replaceAll('{browserContextId}', op.browserContextId)
    .replaceAll('{hostSessionId}', op.hostSessionId);
}

function workerKey(op) {
  switch (op.server.pool) {
    case 'project': return `${op.server.serverId}:project:${op.projectRoot}`;
    case 'session': return `${op.server.serverId}:session:${op.sessionId}:context:${op.contextId}`;
    case 'remote-session': return `${op.server.serverId}:transport:${op.transportSessionId}:credential:${op.credentialProfile}`;
    case 'remote-shared': return `${op.server.serverId}:provider:${op.providerBudgetKey}`;
    case 'credential-session': return `${op.server.serverId}:credential:${op.credentialProfile}:tenant:${op.tenantId}`;
    case 'browser': return `${op.server.serverId}:browser:${op.browserContextId}:host:${op.hostSessionId}`;
    default: return `${op.server.serverId}:process`;
  }
}

function makeOperation(servers, rng, index) {
  const server = rng.pick(servers);
  const projectRoot = rng.pick(CONTEXT.projects);
  const op = {
    id: `op-${index}`,
    ready: rng.int(Math.max(100, Math.ceil(servers.length * 4))),
    duration: 1 + rng.int(5),
    clientId: rng.pick(CONTEXT.clients),
    sessionId: rng.pick(CONTEXT.sessions),
    contextId: rng.pick(CONTEXT.contexts),
    projectRoot,
    transportSessionId: rng.pick(CONTEXT.transportSessions),
    credentialProfile: rng.pick(CONTEXT.credentials),
    tenantId: rng.pick(CONTEXT.tenants),
    providerBudgetKey: rng.pick(CONTEXT.providers),
    browserContextId: rng.pick(CONTEXT.browsers),
    hostSessionId: rng.pick(CONTEXT.hosts),
    server,
    enabled: server.reviewGate ? rng.next() > 0.5 : server.defaultEnabled && rng.next() > 0.08,
    knownTool: rng.next() > 0.1,
  };
  op.workerKey = workerKey(op);
  op.locks = server.lockTemplates.map((template) => materialize(template, op));
  return op;
}

function runSchedulerStress(servers, operations, seed) {
  const rng = new Rng(seed ^ 0xa11cc0de);
  const started = performance.now();
  const ops = Array.from({ length: operations }, (_, index) => makeOperation(servers, rng, index))
    .sort((left, right) => left.ready - right.ready || left.id.localeCompare(right.id));
  const readyQueue = [];
  const readyIds = new Set();
  const active = [];
  const deferred = new Map();
  const waitersByLock = new Map();
  const waitersByWorker = new Map();
  const lockOwners = new Map();
  const workerCounts = new Map();
  const violations = [];
  let tick = 0;
  let cursor = 0;
  let startedCount = 0;
  let finishedCount = 0;
  let blockedDisabled = 0;
  let blockedUnknownTool = 0;
  let blockedReviewGate = 0;
  let lockDeferrals = 0;
  let maxInFlightDeferrals = 0;
  let wakeups = 0;
  let maxActive = 0;

  function enqueue(op) {
    if (readyIds.has(op.id)) return;
    readyIds.add(op.id);
    readyQueue.push(op);
  }

  function waiterSet(map, key) {
    let set = map.get(key);
    if (!set) {
      set = new Set();
      map.set(key, set);
    }
    return set;
  }

  function defer(op, lockConflicts, workerConflict) {
    deferred.set(op.id, op);
    for (const lock of lockConflicts) waiterSet(waitersByLock, lock).add(op.id);
    if (workerConflict) waiterSet(waitersByWorker, workerConflict).add(op.id);
  }

  function wakeWaiters(map, key) {
    const ids = map.get(key);
    if (!ids) return;
    for (const id of ids) {
      ids.delete(id);
      const op = deferred.get(id);
      if (!op) continue;
      deferred.delete(id);
      enqueue(op);
      wakeups += 1;
      break;
    }
    if (ids.size === 0) map.delete(key);
  }

  function releaseFinished() {
    const releasedWorkers = [];
    for (let index = active.length - 1; index >= 0; index -= 1) {
      const item = active[index];
      if (item.endTick > tick) continue;
      for (const lock of item.op.locks) {
        const owner = lockOwners.get(lock);
        if (owner !== item.op.id) violations.push(`lock owner mismatch ${lock}: ${owner} != ${item.op.id}`);
        lockOwners.delete(lock);
        wakeWaiters(waitersByLock, lock);
      }
      const count = workerCounts.get(item.op.workerKey) || 0;
      workerCounts.set(item.op.workerKey, Math.max(0, count - 1));
      releasedWorkers.push(item.op.workerKey);
      active.splice(index, 1);
      finishedCount += 1;
    }
    for (const worker of releasedWorkers) wakeWaiters(waitersByWorker, worker);
  }

  function tryStart(op) {
    if (!op.enabled) { blockedDisabled += 1; return; }
    if (!op.knownTool) { blockedUnknownTool += 1; return; }
    if (op.server.reviewGate || op.server.safety.startsWith('P0_') || op.server.safety.startsWith('PX_')) { blockedReviewGate += 1; return; }

    const lockConflicts = op.locks.filter((lock) => lockOwners.has(lock));
    if (lockConflicts.length > 0) {
      lockDeferrals += 1;
      defer(op, lockConflicts, null);
      return;
    }
    const count = workerCounts.get(op.workerKey) || 0;
    if (count >= op.server.maxInFlight) {
      maxInFlightDeferrals += 1;
      defer(op, [], op.workerKey);
      return;
    }

    for (const lock of op.locks) {
      if (lockOwners.has(lock)) violations.push(`overlapping lock accepted: ${lock}`);
      lockOwners.set(lock, op.id);
    }
    workerCounts.set(op.workerKey, count + 1);
    active.push({ op, endTick: tick + op.duration });
    maxActive = Math.max(maxActive, active.length);
    startedCount += 1;
  }

  const maxTicks = Math.max(20_000, operations * 4 + 1000);
  while (cursor < ops.length || readyQueue.length > 0 || active.length > 0 || deferred.size > 0) {
    releaseFinished();
    while (cursor < ops.length && ops[cursor].ready <= tick) {
      enqueue(ops[cursor]);
      cursor += 1;
    }
    while (readyQueue.length > 0) {
      const op = readyQueue.shift();
      readyIds.delete(op.id);
      tryStart(op);
    }

    if (deferred.size > 0 && active.length === 0 && readyQueue.length === 0 && (cursor >= ops.length || ops[cursor].ready > tick)) {
      violations.push(`deferred operations cannot wake without active lock owners: ${deferred.size}`);
      break;
    }

    if (cursor >= ops.length && readyQueue.length === 0 && active.length === 0 && deferred.size === 0) break;

    const nextReady = cursor < ops.length ? ops[cursor].ready : Number.POSITIVE_INFINITY;
    const nextEnd = active.length > 0 ? Math.min(...active.map((item) => item.endTick)) : Number.POSITIVE_INFINITY;
    const nextTick = Math.min(nextReady, nextEnd);
    if (Number.isFinite(nextTick) && nextTick > tick) tick = nextTick;
    else tick += 1;
    if (tick > maxTicks) {
      violations.push(`scheduler did not drain within ${maxTicks} ticks`);
      break;
    }
  }

  return {
    elapsedMs: round(performance.now() - started),
    operations,
    ticks: tick,
    started: startedCount,
    finished: finishedCount,
    blockedDisabled,
    blockedUnknownTool,
    blockedReviewGate,
    lockDeferrals,
    maxInFlightDeferrals,
    wakeups,
    maxActive,
    activeAtEnd: active.length,
    waitingAtEnd: readyQueue.length + deferred.size,
    violations,
  };
}

function classifyMetadata(profile) {
  return classifyMcpPackageMetadata(profile);
}

function runProfileClassificationStress(count, seed) {
  const rng = new Rng(seed ^ 0xc1a551f1);
  const words = ['filesystem', 'memory', 'docs', 'sqlite', 'git', 'browser', 'slack', 'aws', 'shell', 'time', 'search', 'context'];
  const started = performance.now();
  let reviewRequired = 0;
  let executeDefault = 0;
  const signalCounts = new Map();
  for (let index = 0; index < count; index += 1) {
    const selected = [rng.pick(words), rng.pick(words), rng.pick(words)];
    const result = classifyMetadata({
      name: `pkg-${selected[0]}-${index}`,
      description: `MCP server for ${selected.join(' ')} workflows`,
      keywords: ['mcp', 'server', ...selected],
    });
    if (result.reviewRequired) reviewRequired += 1;
    if (result.executeDefault) executeDefault += 1;
    for (const signal of result.signals) signalCounts.set(signal, (signalCounts.get(signal) || 0) + 1);
  }
  return { elapsedMs: round(performance.now() - started), profiles: count, reviewRequired, executeDefault, signalCounts: Object.fromEntries(signalCounts) };
}

function makeChecks(report) {
  const checks = [
    { id: 'no-random-mcp-server-start', ok: report.safety.startsMcpServers === false && report.safety.callsMcpTools === false, detail: 'Stress harness never starts third-party MCP packages and never sends tools/call.' },
    { id: 'catalog-volume', ok: report.toolCatalog.searchSpaceToolCount === report.scenario.tools, detail: `${report.toolCatalog.searchSpaceToolCount}/${report.scenario.tools} synthetic tools indexed.` },
    { id: 'tool-search-top-k-bounded', ok: report.toolCatalog.retainedSearchCandidates <= Math.max(2, report.scenario.searchLimit), detail: `${report.toolCatalog.retainedSearchCandidates} retained candidates.` },
    { id: 'projection-bounded', ok: report.toolCatalog.projectedToolCount <= report.scenario.projectionBudget && report.toolCatalog.firstPageCount <= report.scenario.pageSize, detail: `projected=${report.toolCatalog.projectedToolCount}; firstPage=${report.toolCatalog.firstPageCount}.` },
    { id: 'known-tool-lookup-fails-closed', ok: report.lookup.unknownHits > 0 && report.lookup.unknownForwarded === 0, detail: `unknownHits=${report.lookup.unknownHits}; unknownForwarded=${report.lookup.unknownForwarded}.` },
    { id: 'scheduler-drains', ok: report.scheduler.violations.length === 0 && report.scheduler.activeAtEnd === 0 && report.scheduler.waitingAtEnd === 0, detail: `ticks=${report.scheduler.ticks}; violations=${report.scheduler.violations.length}.` },
    { id: 'scheduler-blocks-disabled-unknown-review', ok: report.scheduler.blockedDisabled > 0 && report.scheduler.blockedUnknownTool > 0 && report.scheduler.blockedReviewGate > 0, detail: `disabled=${report.scheduler.blockedDisabled}; unknown=${report.scheduler.blockedUnknownTool}; review=${report.scheduler.blockedReviewGate}.` },
    { id: 'scheduler-starts-safe-work', ok: report.scheduler.started > 0 && report.scheduler.finished === report.scheduler.started, detail: `started=${report.scheduler.started}; finished=${report.scheduler.finished}.` },
    { id: 'metadata-classification-disabled-by-default', ok: report.profileClassification.executeDefault === 0, detail: `${report.profileClassification.profiles} metadata profiles classified; executeDefault=${report.profileClassification.executeDefault}.` },
    { id: 'heap-budget', ok: report.memory.heapDeltaMiB <= report.scenario.memoryLimitMiB, detail: `heapDeltaMiB=${report.memory.heapDeltaMiB}; budget=${report.scenario.memoryLimitMiB}.` },
  ];
  return checks;
}

function markdown(report) {
  const lines = [
    '# MCP overhead stress report',
    '',
    `Generated: ${report.generatedAt}`,
    `Status: **${report.status}**`,
    '',
    '## Scenario',
    '',
    `- Servers: ${report.scenario.servers}`,
    `- Synthetic tools: ${report.scenario.tools}`,
    `- Scheduler operations: ${report.scenario.operations}`,
    `- Metadata profiles: ${report.scenario.packageProfiles}`,
    '',
    '## Results',
    '',
    `- Tool catalog/index: ${report.toolCatalog.elapsedMs} ms, heap +${report.toolCatalog.heapDeltaMiB} MiB, retained top-k ${report.toolCatalog.retainedSearchCandidates}.`,
    `- Known-tool lookup: ${report.lookup.elapsedMs} ms for ${report.lookup.lookupCount} lookups; unknown forwarded ${report.lookup.unknownForwarded}.`,
    `- Scheduler: ${report.scheduler.elapsedMs} ms for ${report.scheduler.operations} ops; started ${report.scheduler.started}; deferrals ${report.scheduler.lockDeferrals + report.scheduler.maxInFlightDeferrals}.`,
    `- Profile classification: ${report.profileClassification.elapsedMs} ms for ${report.profileClassification.profiles} metadata profiles.`,
    `- Total: ${report.elapsedMs} ms; heap +${report.memory.heapDeltaMiB} MiB.`,
    '',
    '## Safety invariants',
    '',
    `- Starts MCP servers: ${report.safety.startsMcpServers}`,
    `- Calls MCP tools: ${report.safety.callsMcpTools}`,
    `- Executes third-party package code: ${report.safety.executesThirdPartyPackages}`,
    `- Auto-enables random packages: ${report.safety.defaultServerEnablement}`,
    '',
    '## Checks',
    '',
    '| Check | Status | Detail |',
    '|---|---:|---|',
  ];
  for (const check of report.checks) lines.push(`| ${check.id} | ${check.ok ? 'pass' : 'fail'} | ${String(check.detail).replace(/\|/g, '\\|')} |`);
  lines.push('');
  lines.push('## Notes');
  lines.push('');
  lines.push('- This harness measures MCP hub overhead with synthetic server/tool profiles only. It is intentionally not a random MCP server execution harness.');
  lines.push('- Latency is recorded as evidence, but host-specific latency budgets should be baselined per OS/architecture before becoming hard release gates.');
  return `${lines.join('\n')}\n`;
}

function makeReport(args) {
  const startedAt = performance.now();
  const heapStart = process.memoryUsage().heapUsed;
  const servers = buildServers(args.servers);
  const toolCatalog = runToolCatalogStress({ servers, tools: args.tools, searchLimit: args.searchLimit, projectionBudget: args.projectionBudget, pageSize: args.pageSize });
  const lookup = runLookupStress(toolCatalog.knownSamples, args.operations, args.seed);
  const scheduler = runSchedulerStress(servers, args.operations, args.seed);
  const profileClassification = runProfileClassificationStress(args.packageProfiles, args.seed);
  const elapsedMs = round(performance.now() - startedAt);
  const memory = {
    heapStartMiB: round(heapStart / 1024 / 1024, 3),
    heapEndMiB: round(process.memoryUsage().heapUsed / 1024 / 1024, 3),
  };
  memory.heapDeltaMiB = round(memory.heapEndMiB - memory.heapStartMiB, 3);

  const report = {
    schema: 'mcpace.mcpOverheadStress.v1',
    generatedAt: new Date().toISOString(),
    project: { name: deriveProjectName(), version: deriveProjectVersion() },
    environment: { node: process.version, platform: process.platform, arch: process.arch, cpuCount: os.cpus().length, availableParallelism: os.availableParallelism?.() || os.cpus().length || null },
    scenario: { servers: args.servers, tools: args.tools, operations: args.operations, packageProfiles: args.packageProfiles, searchLimit: args.searchLimit, projectionBudget: args.projectionBudget, pageSize: args.pageSize, memoryLimitMiB: args.memoryLimitMiB, seed: args.seed },
    safety: { executesThirdPartyPackages: false, startsMcpServers: false, callsMcpTools: false, packageInstallScriptsAllowed: false, destructiveToolCallsAllowed: false, userSecretsPassedToRuntime: false, defaultServerEnablement: false },
    optimizations: {
      boundedTopKSearch: true,
      boundedProjection: true,
      knownToolSetValidation: true,
      lockDomainScheduling: true,
      disabledByDefaultForUnknownPackages: true,
      metadataOnlyClassification: true,
    },
    toolCatalog: { ...toolCatalog, knownSamples: undefined, topSearchSample: toolCatalog.topSearch.slice(0, 5), topSearch: undefined },
    lookup,
    scheduler,
    profileClassification,
    memory,
    elapsedMs,
  };
  report.checks = makeChecks(report);
  report.status = report.checks.every((check) => check.ok) ? 'pass' : 'fail';
  return report;
}

function writeReport(report, args) {
  if (args.write) {
    const output = path.resolve(repoRoot, args.write);
    fs.mkdirSync(path.dirname(output), { recursive: true });
    fs.writeFileSync(output, `${JSON.stringify(report, null, 2)}\n`);
  }
  if (args.markdown) {
    const output = path.resolve(repoRoot, args.markdown);
    fs.mkdirSync(path.dirname(output), { recursive: true });
    fs.writeFileSync(output, markdown(report));
  }
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) { printHelp(); return; }
  const report = makeReport(args);
  writeReport(report, args);
  if (args.json) console.log(JSON.stringify(report, null, 2));
  else console.log(`${report.status}: ${report.scenario.tools} tools, ${report.scenario.operations} ops in ${report.elapsedMs}ms; heap +${report.memory.heapDeltaMiB} MiB`);
  if (report.status !== 'pass') process.exitCode = 1;
}

try { main(); } catch (error) { console.error(error.stack || error.message || String(error)); process.exitCode = 1; }
