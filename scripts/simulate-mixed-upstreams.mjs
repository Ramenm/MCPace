#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { performance } from 'node:perf_hooks';
import { pathToFileURL } from 'node:url';

const DEFAULT_MIX = [
  'stdio-ok',
  'http-ok',
  'https-blocked',
  'legacy-sse-blocked',
  'disabled',
  'missing-command',
  'bad-cwd',
  'unsupported-custom',
  'timeout-failed',
  'invalid-json-failed',
];

function parseArgs(argv) {
  const parsed = new Map();
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    if (!token.startsWith('--')) throw new Error(`unsupported argument: ${token}`);
    const key = token.slice(2);
    const next = argv[index + 1];
    if (!next || next.startsWith('--')) {
      parsed.set(key, true);
    } else {
      parsed.set(key, next);
      index += 1;
    }
  }
  return parsed;
}

function numberArg(args, name, fallback, { min = 0, max = Number.MAX_SAFE_INTEGER } = {}) {
  const raw = args.get(name) ?? fallback;
  const value = Number(raw);
  if (!Number.isFinite(value) || value < min || value > max) throw new Error(`invalid --${name}`);
  return Math.floor(value);
}

function normalize(value) {
  return String(value || '').toLowerCase().replace(/[^a-z0-9_.-]+/g, ' ');
}

function statusForKind(kind) {
  switch (kind) {
    case 'stdio-ok': return 'callable-stdio';
    case 'http-ok': return 'callable-http';
    case 'https-blocked': return 'blocked-https-upstream';
    case 'legacy-sse-blocked': return 'blocked-legacy-sse-upstream';
    case 'disabled': return 'disabled';
    case 'missing-command': return 'blocked-command-not-found';
    case 'bad-cwd': return 'blocked-command-not-found';
    case 'unsupported-custom': return 'blocked-unsupported-transport';
    case 'timeout-failed': return 'catalog-failed';
    case 'invalid-json-failed': return 'catalog-failed';
    default: return 'blocked-unsupported-transport';
  }
}

function sourceTypeForKind(kind) {
  switch (kind) {
    case 'stdio-ok':
    case 'missing-command':
    case 'bad-cwd':
    case 'timeout-failed':
    case 'invalid-json-failed':
      return 'stdio';
    case 'http-ok':
    case 'https-blocked':
      return 'http';
    case 'legacy-sse-blocked':
      return 'legacy-sse';
    case 'unsupported-custom':
      return 'custom';
    default:
      return 'stdio';
  }
}

function isCallableSuccess(kind) {
  return kind === 'stdio-ok' || kind === 'http-ok';
}

function isRuntimeFailure(kind) {
  return kind === 'timeout-failed' || kind === 'invalid-json-failed';
}

function serverKind(index, mix) {
  return mix[index % mix.length];
}

function insertTopK(items, item, limit) {
  items.push(item);
  items.sort((left, right) => right.score - left.score || left.key.localeCompare(right.key));
  if (items.length > limit) items.length = limit;
}

function syntheticTool(serverName, serverIndex, toolIndex) {
  const shared = toolIndex % 11 === 0;
  const readOnly = toolIndex % 10 < 7;
  const mutating = !readOnly;
  const family = shared
    ? 'shared_lookup'
    : readOnly
      ? ['read', 'search', 'list', 'describe'][toolIndex % 4]
      : ['write', 'delete', 'update'][toolIndex % 3];
  const toolName = shared ? `shared_lookup_${Math.floor(toolIndex / 11)}` : `${family}_${toolIndex}`;
  return {
    server: serverName,
    name: toolName,
    qualifiedName: `${serverName}.${toolName}`,
    title: `${family} tool ${toolIndex}`,
    description: `${readOnly ? 'Read-only discovery' : 'Mutating state'} tool ${toolIndex} on ${serverName}`,
    readOnly,
    mutating,
  };
}

function scoreTool(tool, terms) {
  if (terms.length === 0) return 1;
  const server = normalize(tool.server);
  const name = normalize(tool.name);
  const qualified = normalize(tool.qualifiedName);
  const title = normalize(tool.title);
  const description = normalize(tool.description);
  const all = `${server} ${name} ${qualified} ${title} ${description}`;
  let score = 0;
  for (const term of terms) {
    if (name === term || qualified === term) score += 80;
    if (name.includes(term)) score += 40;
    if (qualified.includes(term)) score += 30;
    if (title.includes(term)) score += 20;
    if (description.includes(term)) score += 10;
    if (server.includes(term)) score += 5;
    if (all.includes(term)) score += 1;
  }
  return score;
}

function compactTool(tool, score) {
  return {
    server: tool.server,
    name: tool.name,
    qualifiedName: tool.qualifiedName,
    score,
    call: { tool: 'upstream_call', arguments: { server: tool.server, tool: tool.name } },
  };
}

export function runMixedUpstreamSimulation(options = {}) {
  const servers = options.servers ?? 50;
  const tools = options.tools ?? 200_000;
  const searchLimit = Math.max(1, Math.min(options.searchLimit ?? 25, 100));
  const projectionBudget = Math.max(1, Math.min(options.projectionBudget ?? 64, 2048));
  const pageSize = Math.max(1, Math.min(options.pageSize ?? 128, 512));
  const memoryLimitMiB = options.memoryLimitMiB ?? 512;
  const query = options.query ?? 'shared lookup read search';
  const mix = Array.isArray(options.mix) && options.mix.length ? options.mix : DEFAULT_MIX;
  const terms = normalize(query).split(/\s+/).filter((term) => term.length >= 2);

  const started = performance.now();
  const heapStart = process.memoryUsage().heapUsed;
  const topSearch = [];
  const statusCounts = new Map();
  const sourceTypeCounts = new Map();
  const toolNameFirstServer = new Map();
  const qualifiedNames = new Set();
  let duplicateToolNameCollisions = 0;
  let qualifiedNameCollisions = 0;
  let configuredToolSlots = 0;
  let searchableToolCount = 0;
  let matchCount = 0;
  let readOnlyCount = 0;
  let mutatingCount = 0;
  let projectedCandidateCount = 0;
  let projectedToolCount = 0;
  let failedServerCount = 0;
  let blockedServerCount = 0;
  let callableServerCount = 0;
  let degradedServerCount = 0;
  const projectionCandidateLimit = Math.max(projectionBudget, Math.min(projectionBudget * 8, 8192));

  const toolsPerServerBase = Math.floor(tools / Math.max(1, servers));
  const remainder = tools % Math.max(1, servers);

  for (let serverIndex = 0; serverIndex < servers; serverIndex += 1) {
    const kind = serverKind(serverIndex, mix);
    const status = statusForKind(kind);
    const sourceType = sourceTypeForKind(kind);
    statusCounts.set(status, (statusCounts.get(status) ?? 0) + 1);
    sourceTypeCounts.set(sourceType, (sourceTypeCounts.get(sourceType) ?? 0) + 1);
    if (isCallableSuccess(kind)) callableServerCount += 1;
    else if (isRuntimeFailure(kind)) { failedServerCount += 1; degradedServerCount += 1; }
    else blockedServerCount += 1;

    const serverName = `srv-${String(serverIndex).padStart(2, '0')}-${sourceType}`;
    const count = toolsPerServerBase + (serverIndex < remainder ? 1 : 0);
    configuredToolSlots += count;
    if (!isCallableSuccess(kind)) continue;

    for (let toolIndex = 0; toolIndex < count; toolIndex += 1) {
      const tool = syntheticTool(serverName, serverIndex, toolIndex);
      searchableToolCount += 1;
      if (tool.readOnly) readOnlyCount += 1;
      if (tool.mutating) mutatingCount += 1;

      const firstServer = toolNameFirstServer.get(tool.name);
      if (firstServer && firstServer !== tool.server) duplicateToolNameCollisions += 1;
      else if (!firstServer) toolNameFirstServer.set(tool.name, tool.server);

      if (qualifiedNames.has(tool.qualifiedName)) qualifiedNameCollisions += 1;
      else qualifiedNames.add(tool.qualifiedName);

      const score = scoreTool(tool, terms);
      if (terms.length === 0 || score > 0) {
        matchCount += 1;
        insertTopK(
          topSearch,
          { score, key: tool.qualifiedName, tool: compactTool(tool, score) },
          Math.max(2, searchLimit),
        );
      }
      if (tool.readOnly && projectedCandidateCount < projectionCandidateLimit) {
        projectedCandidateCount += 1;
      }
    }
  }

  projectedToolCount = Math.min(projectionBudget, projectedCandidateCount);
  const firstPageCount = Math.min(pageSize, 8 + projectedToolCount);
  const elapsedMs = Math.round(performance.now() - started);
  const heapDeltaMiB = Math.round(((process.memoryUsage().heapUsed - heapStart) / 1024 / 1024) * 10) / 10;

  const requiredStatuses = [
    'callable-stdio',
    'callable-http',
    'blocked-https-upstream',
    'blocked-legacy-sse-upstream',
    'blocked-unsupported-transport',
    'catalog-failed',
    'disabled',
  ];
  const sourceCoverageOk = requiredStatuses.every((status) => (statusCounts.get(status) ?? 0) > 0);
  const pass =
    configuredToolSlots === tools &&
    callableServerCount > 0 &&
    blockedServerCount > 0 &&
    failedServerCount > 0 &&
    topSearch.length <= Math.max(2, searchLimit) &&
    projectedCandidateCount <= projectionCandidateLimit &&
    projectedToolCount <= projectionBudget &&
    firstPageCount <= pageSize &&
    duplicateToolNameCollisions > 0 &&
    qualifiedNameCollisions === 0 &&
    sourceCoverageOk &&
    heapDeltaMiB <= memoryLimitMiB;

  return {
    schema: 'mcpace.mixedUpstreamSimulation.v1',
    status: pass ? 'pass' : 'fail',
    generatedAt: new Date().toISOString(),
    scenario: {
      servers,
      tools,
      query,
      mix,
      searchLimit,
      projectionBudget,
      projectionCandidateLimit,
      pageSize,
      memoryLimitMiB,
    },
    transportMatrix: {
      stdioDirect: true,
      plainStreamableHttpDirect: true,
      httpsDirect: false,
      legacyHttpSseDirect: false,
      customTransportDirect: false,
    },
    results: {
      configuredToolSlots,
      searchableToolCount,
      matchCount,
      returnedSearchCount: Math.min(searchLimit, topSearch.length),
      retainedSearchCandidates: topSearch.length,
      readOnlyCount,
      mutatingCount,
      projectedCandidateCount,
      projectedToolCount,
      firstPageCount,
      callableServerCount,
      blockedServerCount,
      failedServerCount,
      degradedServerCount,
      duplicateToolNameCollisions,
      qualifiedNameCollisions,
    },
    counts: {
      statuses: Object.fromEntries(statusCounts),
      sourceTypes: Object.fromEntries(sourceTypeCounts),
    },
    budgets: {
      searchBounded: topSearch.length <= Math.max(2, searchLimit),
      projectionBounded: projectedCandidateCount <= projectionCandidateLimit,
      pageBounded: firstPageCount <= pageSize,
      failureIsolation: failedServerCount > 0 && callableServerCount > 0,
      collisionSafe: duplicateToolNameCollisions > 0 && qualifiedNameCollisions === 0,
      sourceCoverageOk,
      heapDeltaMiB,
      memoryLimitMiB,
    },
    elapsedMs,
  };
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const report = runMixedUpstreamSimulation({
    servers: numberArg(args, 'servers', 50, { min: 1 }),
    tools: numberArg(args, 'tools', 200_000, { min: 0 }),
    searchLimit: numberArg(args, 'search-limit', 25, { min: 1, max: 1000 }),
    projectionBudget: numberArg(args, 'projection-budget', 64, { min: 1, max: 4096 }),
    pageSize: numberArg(args, 'page-size', 128, { min: 1, max: 4096 }),
    memoryLimitMiB: numberArg(args, 'memory-limit-mib', 512, { min: 1 }),
    query: String(args.get('query') ?? 'shared lookup read search'),
    mix: args.get('mix') ? String(args.get('mix')).split(',').map((item) => item.trim()).filter(Boolean) : DEFAULT_MIX,
  });
  const writePath = args.get('write') ? path.resolve(String(args.get('write'))) : null;
  if (writePath) {
    fs.mkdirSync(path.dirname(writePath), { recursive: true });
    fs.writeFileSync(writePath, `${JSON.stringify(report, null, 2)}\n`, 'utf8');
  }
  if (args.has('json')) process.stdout.write(`${JSON.stringify(report)}\n`);
  else process.stdout.write(`${report.status}: ${report.results.searchableToolCount}/${report.results.configuredToolSlots} searchable tools across ${report.scenario.servers} mixed servers in ${report.elapsedMs}ms; heap +${report.budgets.heapDeltaMiB} MiB\n`);
  if (report.status !== 'pass') process.exitCode = 1;
}

if (process.argv[1] && pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url) {
  try { main(); } catch (error) { process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`); process.exitCode = 1; }
}
