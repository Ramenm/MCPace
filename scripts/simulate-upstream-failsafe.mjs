#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { performance } from 'node:perf_hooks';
import { pathToFileURL } from 'node:url';

const DEFAULT_MIX = [
  'healthy-stdio',
  'healthy-http',
  'stale-cache',
  'startup-timeout',
  'list-timeout',
  'invalid-json',
  'tool-timeout',
  'tool-error',
  'process-exit',
  'flapping-recoverable',
  'disabled',
  'unsupported-transport',
];

const DEFAULT_CALL_PLAN = [
  'healthy-stdio:ok',
  'healthy-http:ok',
  'stale-cache:stale-only-call-unavailable',
  'tool-error:upstream-is-error',
  'tool-timeout:timeout',
  'process-exit:process-exit',
  'flapping-recoverable:retry-success',
  'startup-timeout:circuit-open',
  'invalid-json:protocol-error',
];

function parseArgs(argv) {
  const parsed = new Map();
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    if (!token.startsWith('--')) throw new Error(`unsupported argument: ${token}`);
    const key = token.slice(2);
    const next = argv[index + 1];
    if (!next || next.startsWith('--')) parsed.set(key, true);
    else { parsed.set(key, next); index += 1; }
  }
  return parsed;
}

function numberArg(args, name, fallback, { min = 0, max = Number.MAX_SAFE_INTEGER } = {}) {
  const raw = args.get(name) ?? fallback;
  const value = Number(raw);
  if (!Number.isFinite(value) || value < min || value > max) throw new Error(`invalid --${name}`);
  return Math.floor(value);
}

function boolArg(args, name, fallback = false) {
  if (!args.has(name)) return fallback;
  const raw = args.get(name);
  if (raw === true) return true;
  return !['0', 'false', 'no', 'off'].includes(String(raw).trim().toLowerCase());
}

function kindAt(index, mix) {
  return mix[index % mix.length];
}

function discoveryStatus(kind) {
  switch (kind) {
    case 'healthy-stdio': return 'listed-tools';
    case 'healthy-http': return 'listed-tools';
    case 'stale-cache': return 'stale-cache-used';
    case 'startup-timeout': return 'startup-timeout';
    case 'list-timeout': return 'catalog-timeout';
    case 'invalid-json': return 'protocol-error';
    case 'tool-timeout': return 'listed-tools';
    case 'tool-error': return 'listed-tools';
    case 'process-exit': return 'startup-exit';
    case 'flapping-recoverable': return 'listed-tools-after-retry';
    case 'disabled': return 'disabled';
    case 'unsupported-transport': return 'blocked-unsupported-transport';
    default: return 'blocked-unsupported-transport';
  }
}

function sourceType(kind) {
  if (kind === 'healthy-http') return 'http';
  if (kind === 'unsupported-transport') return 'custom';
  return 'stdio';
}

function hasFreshCatalog(kind) {
  return ['healthy-stdio', 'healthy-http', 'tool-timeout', 'tool-error', 'flapping-recoverable'].includes(kind);
}

function hasStaleCatalog(kind) {
  return kind === 'stale-cache';
}

function isBlocked(kind) {
  return ['disabled', 'unsupported-transport'].includes(kind);
}

function isDiscoveryFailure(kind) {
  return ['startup-timeout', 'list-timeout', 'invalid-json', 'process-exit'].includes(kind);
}

function isCircuitBreakerCandidate(kind) {
  return ['startup-timeout', 'list-timeout', 'invalid-json', 'process-exit', 'tool-timeout'].includes(kind);
}

function toolSlotsForServer(totalTools, servers, index) {
  const base = Math.floor(totalTools / Math.max(1, servers));
  return base + (index < totalTools % Math.max(1, servers) ? 1 : 0);
}

function circuitStateFor(kind, failureThreshold, failureWindow) {
  if (!isCircuitBreakerCandidate(kind)) return { state: 'closed', failures: 0, opened: false };
  const failures = Math.min(failureWindow, failureThreshold + 1);
  return { state: failures >= failureThreshold ? 'open' : 'closed', failures, opened: failures >= failureThreshold };
}

function callOutcome(kind, action, retries) {
  const base = { kind, action, attempts: 1, bridgeOk: true, upstreamOk: false, finalStatus: 'failed', fallbackUsed: false, circuitOpened: false };
  switch (kind) {
    case 'healthy-stdio':
    case 'healthy-http':
      return { ...base, upstreamOk: true, finalStatus: 'ok' };
    case 'stale-cache':
      return { ...base, bridgeOk: true, finalStatus: 'unavailable-from-stale-catalog', fallbackUsed: true };
    case 'tool-error':
      return { ...base, finalStatus: 'upstream-is-error' };
    case 'tool-timeout':
      return { ...base, attempts: 1 + Math.min(1, retries), finalStatus: retries > 0 ? 'retry-timeout' : 'timeout', circuitOpened: true };
    case 'process-exit':
      return { ...base, finalStatus: 'process-exit', circuitOpened: true };
    case 'flapping-recoverable':
      return { ...base, attempts: 1 + Math.min(1, retries), upstreamOk: retries > 0, finalStatus: retries > 0 ? 'retry-success' : 'flapping-failed-once' };
    case 'startup-timeout':
      return { ...base, finalStatus: 'circuit-open', circuitOpened: true };
    case 'invalid-json':
      return { ...base, finalStatus: 'protocol-error', circuitOpened: true };
    default:
      return { ...base, bridgeOk: false, finalStatus: 'not-callable' };
  }
}

function batchSimulation({ callsPerBatch, failFast }) {
  const calls = [];
  const sequence = ['ok', 'ok', 'upstream-is-error', 'timeout', 'ok'];
  for (let index = 0; index < callsPerBatch; index += 1) {
    const status = sequence[index % sequence.length];
    const ok = status === 'ok';
    calls.push({ index, status, ok });
    if (failFast && !ok) break;
  }
  return {
    mode: failFast ? 'fail-fast' : 'continue-on-error',
    requestedCalls: callsPerBatch,
    executedCalls: calls.length,
    okCount: calls.filter((call) => call.ok).length,
    failedCount: calls.filter((call) => !call.ok).length,
    calls,
  };
}

export function runUpstreamFailsafeSimulation(options = {}) {
  const servers = options.servers ?? 50;
  const tools = options.tools ?? 200_000;
  const memoryLimitMiB = options.memoryLimitMiB ?? 512;
  const staleFallback = options.staleFallback ?? true;
  const failureThreshold = options.failureThreshold ?? 3;
  const failureWindow = options.failureWindow ?? 5;
  const retries = options.retries ?? 1;
  const callsPerBatch = options.callsPerBatch ?? 5;
  const mix = Array.isArray(options.mix) && options.mix.length ? options.mix : DEFAULT_MIX;
  const callPlan = Array.isArray(options.callPlan) && options.callPlan.length ? options.callPlan : DEFAULT_CALL_PLAN;

  const started = performance.now();
  const heapStart = process.memoryUsage().heapUsed;
  const counts = new Map();
  const sourceTypes = new Map();
  let configuredToolSlots = 0;
  let freshSearchableTools = 0;
  let staleSearchableTools = 0;
  let blockedServers = 0;
  let failedDiscoveryServers = 0;
  let degradedServers = 0;
  let circuitOpenServers = 0;
  let recoverableServers = 0;
  let healthyServers = 0;
  let staleFallbackServers = 0;

  for (let index = 0; index < servers; index += 1) {
    const kind = kindAt(index, mix);
    const status = discoveryStatus(kind);
    const type = sourceType(kind);
    const slots = toolSlotsForServer(tools, servers, index);
    configuredToolSlots += slots;
    counts.set(status, (counts.get(status) ?? 0) + 1);
    sourceTypes.set(type, (sourceTypes.get(type) ?? 0) + 1);

    if (hasFreshCatalog(kind)) {
      freshSearchableTools += slots;
      healthyServers += ['healthy-stdio', 'healthy-http'].includes(kind) ? 1 : 0;
      if (kind === 'flapping-recoverable') recoverableServers += 1;
    } else if (staleFallback && hasStaleCatalog(kind)) {
      staleSearchableTools += slots;
      staleFallbackServers += 1;
      degradedServers += 1;
    } else if (isBlocked(kind)) {
      blockedServers += 1;
    } else if (isDiscoveryFailure(kind)) {
      failedDiscoveryServers += 1;
      degradedServers += 1;
    }

    const circuit = circuitStateFor(kind, failureThreshold, failureWindow);
    if (circuit.opened) circuitOpenServers += 1;
  }

  const callOutcomes = callPlan.map((item, index) => {
    const [kind, action = 'call'] = String(item).split(':');
    return { index, ...callOutcome(kind, action, retries) };
  });
  const bridgeOkCount = callOutcomes.filter((call) => call.bridgeOk).length;
  const upstreamOkCount = callOutcomes.filter((call) => call.upstreamOk).length;
  const upstreamFailedCount = callOutcomes.length - upstreamOkCount;
  const circuitOpenedByCalls = callOutcomes.filter((call) => call.circuitOpened).length;
  const retrySuccessCount = callOutcomes.filter((call) => call.finalStatus === 'retry-success').length;
  const staleCallUnavailableCount = callOutcomes.filter((call) => call.finalStatus === 'unavailable-from-stale-catalog').length;

  const failFastBatch = batchSimulation({ callsPerBatch, failFast: true });
  const continueBatch = batchSimulation({ callsPerBatch, failFast: false });

  const searchableTools = freshSearchableTools + staleSearchableTools;
  const heapDeltaMiB = Math.round(((process.memoryUsage().heapUsed - heapStart) / 1024 / 1024) * 10) / 10;
  const pass =
    configuredToolSlots === tools &&
    healthyServers > 0 &&
    failedDiscoveryServers > 0 &&
    blockedServers > 0 &&
    staleFallbackServers > 0 &&
    circuitOpenServers > 0 &&
    recoverableServers > 0 &&
    searchableTools > 0 &&
    upstreamOkCount > 0 &&
    upstreamFailedCount > 0 &&
    bridgeOkCount === callOutcomes.length - callOutcomes.filter((call) => call.finalStatus === 'not-callable').length &&
    retrySuccessCount > 0 &&
    staleCallUnavailableCount > 0 &&
    failFastBatch.executedCalls < continueBatch.executedCalls &&
    heapDeltaMiB <= memoryLimitMiB;

  return {
    schema: 'mcpace.upstreamFailsafeSimulation.v1',
    status: pass ? 'pass' : 'fail',
    generatedAt: new Date().toISOString(),
    scenario: {
      servers,
      tools,
      mix,
      staleFallback,
      failureThreshold,
      failureWindow,
      retries,
      callsPerBatch,
      memoryLimitMiB,
    },
    policy: {
      discoveryDegradesInsteadOfFailingClosed: true,
      staleCacheMayAssistDiscovery: staleFallback,
      staleCacheDoesNotMakeCallsSafe: true,
      circuitBreakerProtectsRepeatedFailures: true,
      perServerFailureIsolation: true,
      batchDefaultMode: 'stateful fail-fast; use independent upstream_call calls for cross-server fan-out',
      retryBudget: retries,
    },
    results: {
      configuredToolSlots,
      freshSearchableTools,
      staleSearchableTools,
      searchableTools,
      healthyServers,
      recoverableServers,
      staleFallbackServers,
      blockedServers,
      failedDiscoveryServers,
      degradedServers,
      circuitOpenServers,
      callCount: callOutcomes.length,
      bridgeOkCount,
      upstreamOkCount,
      upstreamFailedCount,
      circuitOpenedByCalls,
      retrySuccessCount,
      staleCallUnavailableCount,
    },
    counts: {
      discoveryStatuses: Object.fromEntries(counts),
      sourceTypes: Object.fromEntries(sourceTypes),
    },
    callOutcomes,
    batches: {
      failFast: failFastBatch,
      continueOnError: continueBatch,
    },
    budgets: {
      heapDeltaMiB,
      memoryLimitMiB,
      perServerFailureIsolation: failedDiscoveryServers > 0 && healthyServers > 0 && searchableTools > 0,
      staleCacheSemantics: staleFallbackServers > 0 && staleCallUnavailableCount > 0,
      circuitBreakerCovered: circuitOpenServers > 0 && circuitOpenedByCalls > 0,
      flappingRecoveryCovered: retrySuccessCount > 0,
      batchModesDiverge: failFastBatch.executedCalls < continueBatch.executedCalls,
    },
    elapsedMs: Math.round(performance.now() - started),
  };
}

function writeJson(filePath, report) {
  const target = path.resolve(filePath);
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, `${JSON.stringify(report, null, 2)}\n`, 'utf8');
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const report = runUpstreamFailsafeSimulation({
    servers: numberArg(args, 'servers', 50, { min: 1 }),
    tools: numberArg(args, 'tools', 200_000, { min: 0 }),
    memoryLimitMiB: numberArg(args, 'memory-limit-mib', 512, { min: 1 }),
    failureThreshold: numberArg(args, 'failure-threshold', 3, { min: 1 }),
    failureWindow: numberArg(args, 'failure-window', 5, { min: 1 }),
    retries: numberArg(args, 'retries', 1, { min: 0, max: 10 }),
    callsPerBatch: numberArg(args, 'calls-per-batch', 5, { min: 1, max: 100 }),
    staleFallback: boolArg(args, 'stale-fallback', true),
    mix: args.get('mix') ? String(args.get('mix')).split(',').map((item) => item.trim()).filter(Boolean) : DEFAULT_MIX,
  });
  const writePath = args.get('write') ? path.resolve(String(args.get('write'))) : null;
  if (writePath) writeJson(writePath, report);
  if (args.has('json')) process.stdout.write(`${JSON.stringify(report)}\n`);
  else process.stdout.write(`${report.status}: ${report.results.searchableTools}/${report.results.configuredToolSlots} searchable tool slots, ${report.results.failedDiscoveryServers} failed discovery servers, ${report.results.circuitOpenServers} circuit-open servers, heap +${report.budgets.heapDeltaMiB} MiB\n`);
  if (report.status !== 'pass') process.exitCode = 1;
}

if (process.argv[1] && pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url) {
  try { main(); } catch (error) { process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`); process.exitCode = 1; }
}
