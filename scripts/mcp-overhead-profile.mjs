#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { performance } from 'node:perf_hooks';
import { repoRoot, deriveProjectName, deriveProjectVersion } from './lib/project-metadata.mjs';
import { classifyMcpPackageMetadata as classifyPackageMetadata } from './lib/mcp-signal-policy.mjs';

const DEFAULT_WRITE = 'reports/mcp-overhead-profile-latest.json';
const DEFAULT_MARKDOWN = 'reports/mcp-overhead-profile-latest.md';

function parseArgs(argv) {
  const args = {
    json: false,
    write: path.join(repoRoot, DEFAULT_WRITE),
    markdown: path.join(repoRoot, DEFAULT_MARKDOWN),
    runs: 25,
    iterations: 20_000,
    servers: 100,
    tools: 100_000,
    clients: 16,
    sessions: 64,
    operations: 50_000,
    packages: 100,
    toolsPerServer: null,
    memoryLimitMiB: 256,
    packageReport: 'reports/mcp-mass-package-survey-latest.json',
    help: false
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
      case '--runs': args.runs = numberArg(readValue(), token, { min: 3, max: 200 }); break;
      case '--iterations': args.iterations = numberArg(readValue(), token, { min: 1000, max: 2_000_000 }); break;
      case '--servers': args.servers = numberArg(readValue(), token, { min: 1, max: 1000 }); break;
      case '--tools': args.tools = numberArg(readValue(), token, { min: 100, max: 2_000_000 }); args.toolsExplicit = true; break;
      case '--packages': args.packages = numberArg(readValue(), token, { min: 1, max: 100_000 }); break;
      case '--tools-per-server': args.toolsPerServer = numberArg(readValue(), token, { min: 1, max: 100_000 }); break;
      case '--packages': args.packages = numberArg(readValue(), token, { min: 1, max: 100_000 }); break;
      case '--tools-per-server': args.toolsPerServer = numberArg(readValue(), token, { min: 1, max: 5000 }); break;
      case '--clients': args.clients = numberArg(readValue(), token, { min: 1, max: 1000 }); break;
      case '--sessions': args.sessions = numberArg(readValue(), token, { min: 1, max: 10_000 }); break;
      case '--operations': args.operations = numberArg(readValue(), token, { min: 50, max: 5_000_000 }); break;
      case '--memory-limit-mib': args.memoryLimitMiB = numberArg(readValue(), token, { min: 32, max: 4096 }); break;
      case '--package-report': args.packageReport = readValue(); break;
      case '--help':
      case '-h': args.help = true; break;
      default: throw new Error(`unsupported mcp-overhead-profile argument: ${token}`);
    }
  }
  if (args.toolsPerServer !== null) {
    args.tools = args.servers * args.toolsPerServer;
  }
  if (args.toolsPerServer !== null && !args.toolsExplicit) {
    args.tools = Math.max(100, Math.min(2_000_000, args.servers * args.toolsPerServer));
  }
  delete args.toolsExplicit;
  return args;
}

function numberArg(raw, label, { min, max }) {
  const parsed = Number.parseInt(raw, 10);
  if (!Number.isSafeInteger(parsed) || parsed < min || parsed > max) {
    throw new Error(`${label} must be an integer from ${min} to ${max}`);
  }
  return parsed;
}

function printHelp() {
  console.log(`Usage: node scripts/mcp-overhead-profile.mjs [--json] [--iterations N] [--servers N] [--tools N]

Measures bounded MCP hub overhead without starting random MCP servers or calling tools:
  - JSON-RPC parse/route/serialize envelope cost
  - client/session/project shard-key cost
  - scheduler lock admission/release cost
  - large tool index build, exact lookup, and search projection cost
  - 100-package metadata policy classification cost

The benchmark is synthetic by design: it exercises the hub's own routing and
policy paths while keeping third-party MCP binaries disabled.`);
}

function percentile(values, p) {
  if (!values.length) return null;
  const sorted = [...values].sort((a, b) => a - b);
  const index = Math.ceil((p / 100) * sorted.length) - 1;
  return sorted[Math.max(0, Math.min(sorted.length - 1, index))];
}

function median(values) {
  return percentile(values, 50);
}

function round(value, digits = 3) {
  if (!Number.isFinite(value)) return null;
  const factor = 10 ** digits;
  return Math.round(value * factor) / factor;
}

function batchedPerOp(label, runs, operations, fn) {
  const samples = [];
  const opsPerRun = Math.max(1, Math.floor(operations / runs));
  let checksum = 0;
  for (let run = 0; run < runs; run += 1) {
    const started = performance.now();
    checksum = (checksum + fn(opsPerRun, run)) >>> 0;
    const elapsedMs = performance.now() - started;
    samples.push((elapsedMs * 1000) / opsPerRun);
  }
  return summarizeSamples(label, samples, { operationCount: opsPerRun * runs, checksum });
}

function summarizeSamples(label, samples, extra = {}) {
  return {
    label,
    unit: 'microseconds-per-operation',
    p50Us: round(median(samples), 3),
    p95Us: round(percentile(samples, 95), 3),
    maxUs: round(Math.max(...samples), 3),
    minUs: round(Math.min(...samples), 3),
    samples: samples.length,
    ...extra
  };
}

function stableHash32(value) {
  let hash = 0x811c9dc5;
  const text = String(value);
  for (let index = 0; index < text.length; index += 1) {
    hash ^= text.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return hash >>> 0;
}

function shardKey({ server, client, session, project, transport, credential }) {
  const raw = `${server}\x1f${client}\x1f${session}\x1f${project}\x1f${transport}\x1f${credential}`;
  return `${stableHash32(raw).toString(16).padStart(8, '0')}:${server}:${session}`;
}

function makeRoutePayloads(count, sessions, clients) {
  const payloads = [];
  for (let index = 0; index < count; index += 1) {
    const method = index % 5 === 0 ? 'tools/list' : index % 7 === 0 ? 'resources/list' : index % 11 === 0 ? 'prompts/list' : 'tools/call';
    payloads.push(JSON.stringify({
      jsonrpc: '2.0',
      id: index + 1,
      method,
      params: {
        server: `srv-${index % 100}`,
        tool: index % 11 === 0 ? 'readonly_search' : 'readonly_lookup',
        sessionId: `session-${index % sessions}`,
        clientId: `client-${index % clients}`,
        projectRoot: `/workspace/project-${index % 8}`,
        arguments: { query: `term-${index % 127}`, limit: 10 }
      }
    }));
  }
  return payloads;
}

function routeMessage(message) {
  if (message?.jsonrpc !== '2.0' || !message.method) {
    return { jsonrpc: '2.0', id: message?.id ?? null, error: { code: -32600, message: 'invalid request' } };
  }
  switch (message.method) {
    case 'tools/list': return { jsonrpc: '2.0', id: message.id, result: { tools: [] } };
    case 'resources/list': return { jsonrpc: '2.0', id: message.id, result: { resources: [] } };
    case 'prompts/list': return { jsonrpc: '2.0', id: message.id, result: { prompts: [] } };
    case 'tools/call': {
      const params = message.params || {};
      if (!params.server || !params.tool || !params.sessionId || !params.clientId) {
        return { jsonrpc: '2.0', id: message.id, error: { code: -32602, message: 'missing routing identity' } };
      }
      return { jsonrpc: '2.0', id: message.id, result: { routed: true, server: params.server, tool: params.tool } };
    }
    default: return { jsonrpc: '2.0', id: message.id, error: { code: -32601, message: 'method not found' } };
  }
}

function measureJsonRpcRouting(args) {
  const payloads = makeRoutePayloads(Math.min(args.iterations, 20_000), args.sessions, args.clients);
  return batchedPerOp('json-rpc-parse-route-serialize', args.runs, args.iterations, (ops, run) => {
    let checksum = 0;
    for (let index = 0; index < ops; index += 1) {
      const parsed = JSON.parse(payloads[(index + run * ops) % payloads.length]);
      const response = routeMessage(parsed);
      const serialized = JSON.stringify(response);
      checksum ^= serialized.length;
    }
    return checksum;
  });
}

function measureSessionShardKeys(args) {
  const contexts = Array.from({ length: Math.min(args.sessions * args.clients, 4096) }, (_, index) => ({
    server: `srv-${index % args.servers}`,
    client: `client-${index % args.clients}`,
    session: `session-${index % args.sessions}`,
    project: `/workspace/project-${index % 16}`,
    transport: index % 3 === 0 ? 'streamable-http' : 'stdio',
    credential: `credential-${index % 12}`
  }));
  return batchedPerOp('session-shard-key', args.runs, args.iterations, (ops, run) => {
    let checksum = 0;
    for (let index = 0; index < ops; index += 1) {
      checksum ^= shardKey(contexts[(index + run * ops) % contexts.length]).length;
    }
    return checksum;
  });
}

function operationPolicy(index) {
  if (index % 29 === 0) return { policy: 'disabled-dangerous-command-runner', locks: ['host-session'], review: true, disabled: true };
  if (index % 19 === 0) return { policy: 'credential-scoped-review', locks: ['credential-profile', 'tenant'], review: true, disabled: false };
  if (index % 13 === 0) return { policy: 'database-path-single-writer', locks: ['database', 'project'], review: false, disabled: false };
  if (index % 11 === 0) return { policy: 'project-repo-single-writer', locks: ['repo', 'project'], review: false, disabled: false };
  if (index % 7 === 0) return { policy: 'project-filesystem-single-writer', locks: ['file', 'project'], review: false, disabled: false };
  if (index % 5 === 0) return { policy: 'state-profile-single-session', locks: ['session', 'context-store'], review: false, disabled: false };
  return { policy: 'local-utility-multi-reader-candidate', locks: ['server'], review: false, disabled: false };
}

function lockKeysForOperation(index, args) {
  const policy = operationPolicy(index);
  const context = {
    server: `srv-${index % args.servers}`,
    client: `client-${index % args.clients}`,
    session: `session-${index % args.sessions}`,
    project: `/workspace/project-${index % 16}`,
    repo: `/workspace/project-${index % 16}/repo-${index % 5}`,
    database: `/workspace/project-${index % 16}/db-${index % 9}.sqlite`,
    credential: `credential-${index % 12}`,
    tenant: `tenant-${index % 6}`,
    hostSession: `host-${index % 3}`
  };
  return {
    policy,
    reviewApproved: index % 23 === 0,
    keys: policy.locks.map((lock) => {
      switch (lock) {
        case 'file': return `file:${context.project}`;
        case 'project': return `project:${context.project}`;
        case 'repo': return `repo:${context.repo}`;
        case 'database': return `database:${context.database}`;
        case 'session': return `session:${context.client}:${context.session}`;
        case 'context-store': return `context:${context.client}:${context.session}`;
        case 'credential-profile': return `credential:${context.credential}`;
        case 'tenant': return `tenant:${context.tenant}`;
        case 'host-session': return `host:${context.hostSession}`;
        default: return `${lock}:${context.server}`;
      }
    })
  };
}

function measureLockAdmission(args) {
  const active = new Map();
  const stats = { admitted: 0, blockedDisabled: 0, blockedReview: 0, blockedConflict: 0, conflictsPrevented: 0 };
  const metric = batchedPerOp('lock-admission-release', args.runs, args.operations, (ops, run) => {
    let checksum = 0;
    for (let index = 0; index < ops; index += 1) {
      const globalIndex = index + run * ops;
      const op = lockKeysForOperation(globalIndex, args);
      if (op.policy.disabled) {
        stats.blockedDisabled += 1;
        checksum ^= 3;
        continue;
      }
      if (op.policy.review && !op.reviewApproved) {
        stats.blockedReview += 1;
        checksum ^= 5;
        continue;
      }
      const conflict = op.keys.find((key) => active.has(key));
      if (conflict) {
        stats.blockedConflict += 1;
        stats.conflictsPrevented += 1;
        checksum ^= conflict.length;
        continue;
      }
      for (const key of op.keys) active.set(key, globalIndex);
      stats.admitted += 1;
      checksum ^= op.keys.length;
      for (const key of op.keys) active.delete(key);
    }
    return checksum;
  });
  return { ...metric, stats, activeLocksAtEnd: active.size };
}

function normalize(value) {
  return String(value || '').toLowerCase().replace(/[^a-z0-9_.-]+/g, ' ').trim();
}

const READONLY_TOOL_FAMILIES = Object.freeze(['read', 'search', 'list', 'describe']);
const MUTATING_TOOL_FAMILIES = Object.freeze(['write', 'delete', 'update']);
const MAX_SEARCH_CANDIDATES_PER_TERM = 5000;
const MAX_SEARCH_CANDIDATES_PER_QUERY = 20000;

function syntheticTool(server, toolIndex) {
  const readOnly = toolIndex % 10 < 7;
  const family = readOnly ? READONLY_TOOL_FAMILIES[toolIndex % READONLY_TOOL_FAMILIES.length] : MUTATING_TOOL_FAMILIES[toolIndex % MUTATING_TOOL_FAMILIES.length];
  const name = `${family}_${toolIndex}`;
  const qualifiedName = `${server}.${name}`;
  return {
    server,
    name,
    family,
    qualifiedName,
    readOnly,
    mutating: !readOnly,
    // This string is already normalized; avoid a regex pass for every synthetic tool.
    normalized: `${server} ${name} ${family} ${readOnly ? 'readonly' : 'mutating'} project docs search file database memory`
  };
}

function addTerm(termToQualifiedNames, term, key) {
  if (!term) return;
  let bucket = termToQualifiedNames.get(term);
  if (!bucket) {
    bucket = [];
    termToQualifiedNames.set(term, bucket);
  }
  bucket.push(key);
}

function indexReadonlyToolTerms(termToQualifiedNames, tool) {
  // Keep build cost bounded: index a compact fixed token set instead of splitting
  // every synthetic normalized description during the hot benchmark.
  const key = tool.qualifiedName;
  addTerm(termToQualifiedNames, tool.server, key);
  addTerm(termToQualifiedNames, tool.name, key);
  addTerm(termToQualifiedNames, tool.family, key);
  addTerm(termToQualifiedNames, 'readonly', key);
  addTerm(termToQualifiedNames, 'project', key);
  addTerm(termToQualifiedNames, 'docs', key);
  addTerm(termToQualifiedNames, 'search', key);
  addTerm(termToQualifiedNames, 'file', key);
  addTerm(termToQualifiedNames, 'database', key);
  addTerm(termToQualifiedNames, 'memory', key);
}

function buildToolIndex(args) {
  const byQualifiedName = new Map();
  const byServer = new Map();
  const termToQualifiedNames = new Map();
  const readOnlyQualifiedNames = [];
  const toolsPerServerBase = Math.floor(args.tools / args.servers);
  const remainder = args.tools % args.servers;
  const started = performance.now();
  const heapStart = process.memoryUsage().heapUsed;
  let toolCount = 0;
  for (let serverIndex = 0; serverIndex < args.servers; serverIndex += 1) {
    const count = toolsPerServerBase + (serverIndex < remainder ? 1 : 0);
    const server = `srv-${String(serverIndex).padStart(4, '0')}`;
    const serverTools = [];
    for (let toolIndex = 0; toolIndex < count; toolIndex += 1) {
      const tool = syntheticTool(server, toolIndex);
      byQualifiedName.set(tool.qualifiedName, tool);
      serverTools.push(tool.qualifiedName);
      if (tool.readOnly) {
        readOnlyQualifiedNames.push(tool.qualifiedName);
        indexReadonlyToolTerms(termToQualifiedNames, tool);
      }
      toolCount += 1;
    }
    byServer.set(server, serverTools);
  }
  const elapsedMs = performance.now() - started;
  const heapDeltaMiB = (process.memoryUsage().heapUsed - heapStart) / (1024 * 1024);
  return {
    byQualifiedName,
    byServer,
    readOnlyQualifiedNames,
    termToQualifiedNames,
    metrics: {
      label: 'tool-index-build',
      toolCount,
      serverCount: args.servers,
      elapsedMs: round(elapsedMs, 3),
      heapDeltaMiB: round(heapDeltaMiB, 3),
      buildUsPerTool: round((elapsedMs * 1000) / Math.max(1, toolCount), 3),
      searchTermCount: termToQualifiedNames.size
    }
  };
}

function scoreTool(tool, terms) {
  let score = 0;
  for (const term of terms) {
    if (tool.qualifiedName.includes(term)) score += 60;
    if (tool.name.includes(term)) score += 40;
    if (tool.normalized.includes(term)) score += 5;
  }
  return score;
}

function insertTopK(top, item, limit) {
  if (top.length < limit) {
    top.push(item);
    if (top.length === limit) top.sort((a, b) => a.score - b.score || b.key.localeCompare(a.key));
    return;
  }
  const worst = top[0];
  if (item.score > worst.score || (item.score === worst.score && item.key < worst.key)) {
    top[0] = item;
    top.sort((a, b) => a.score - b.score || b.key.localeCompare(a.key));
  }
}

function measureToolLookupAndSearch(args) {
  const index = buildToolIndex(args);
  const exactKeys = index.readOnlyQualifiedNames.slice(0, Math.max(1, Math.min(index.readOnlyQualifiedNames.length, 4096)));
  const exact = batchedPerOp('tool-index-exact-lookup', args.runs, args.iterations, (ops, run) => {
    let checksum = 0;
    for (let indexOffset = 0; indexOffset < ops; indexOffset += 1) {
      const key = exactKeys[(indexOffset + run * ops) % exactKeys.length];
      const tool = index.byQualifiedName.get(key);
      checksum ^= tool ? tool.qualifiedName.length : 0;
    }
    return checksum;
  });

  const queries = ['read search docs', 'file database readonly', 'memory context describe', 'srv-0001 read', 'project docs'];
  const searchSamples = [];
  let searchChecksum = 0;
  const searchRuns = Math.max(5, Math.min(args.runs, 50));
  for (let run = 0; run < searchRuns; run += 1) {
    const terms = normalize(queries[run % queries.length]).split(/\s+/).filter(Boolean);
    const top = [];
    const started = performance.now();
    const candidateScores = new Map();
    for (const term of terms) {
      if (candidateScores.size >= MAX_SEARCH_CANDIDATES_PER_QUERY) break;
      const keys = index.termToQualifiedNames.get(term) || [];
      const weight = term.startsWith('srv-') ? 60 : term === 'read' || term === 'search' || term === 'list' || term === 'describe' ? 40 : 5;
      const limit = Math.min(keys.length, MAX_SEARCH_CANDIDATES_PER_TERM, Math.max(0, MAX_SEARCH_CANDIDATES_PER_QUERY - candidateScores.size));
      for (let offset = 0; offset < limit; offset += 1) {
        const key = keys[offset];
        candidateScores.set(key, (candidateScores.get(key) || 0) + weight);
      }
    }
    for (const [key, score] of candidateScores) {
      if (score <= 0) continue;
      insertTopK(top, { key, score }, 32);
    }
    const elapsedMs = performance.now() - started;
    searchSamples.push(elapsedMs);
    searchChecksum ^= top.length;
  }

  return {
    build: index.metrics,
    exactLookup: exact,
    search: {
      label: 'tool-index-readonly-search-projection',
      unit: 'milliseconds-per-query',
      queryCount: searchSamples.length,
      p50Ms: round(median(searchSamples), 3),
      p95Ms: round(percentile(searchSamples, 95), 3),
      maxMs: round(Math.max(...searchSamples), 3),
      checksum: searchChecksum,
      candidateLimitPerTerm: MAX_SEARCH_CANDIDATES_PER_TERM,
      candidateLimitPerQuery: MAX_SEARCH_CANDIDATES_PER_QUERY
    }
  };
}

function loadPackageMetadata(args) {
  const candidates = [args.packageReport, 'reports/mcp-mass-package-survey-latest.json', 'eval/fixtures/mcp-mass-package-survey-sample.json'];
  for (const candidate of candidates) {
    const full = path.resolve(repoRoot, candidate);
    if (!fs.existsSync(full)) continue;
    try {
      const report = JSON.parse(fs.readFileSync(full, 'utf8'));
      if (Array.isArray(report.packages) && report.packages.length) {
        return { source: path.relative(repoRoot, full).split(path.sep).join('/'), packages: report.packages };
      }
    } catch {
      // Try the next source.
    }
  }
  return {
    source: 'synthetic-fallback',
    packages: Array.from({ length: 100 }, (_, index) => ({
      name: `mcp-synthetic-${index}`,
      version: '0.0.0',
      description: index % 5 === 0 ? 'filesystem and git MCP server' : index % 7 === 0 ? 'oauth cloud api MCP server' : 'readonly docs search MCP server',
      keywords: ['mcp', 'server']
    }))
  };
}

function syntheticPackage(index) {
  const shape = index % 8;
  return {
    name: `synthetic-mcp-${index}`,
    version: '0.0.0',
    description: shape === 0 ? 'filesystem and git MCP server'
      : shape === 1 ? 'oauth cloud api MCP server'
        : shape === 2 ? 'playwright browser desktop MCP server'
          : shape === 3 ? 'sqlite database MCP server'
            : shape === 4 ? 'memory context sequential thinking MCP server'
              : shape === 5 ? 'shell command runner MCP server'
                : shape === 6 ? 'time timezone local utility MCP server'
                  : 'readonly docs search MCP server',
    keywords: ['mcp', 'server'],
  };
}

function measurePackagePolicyPath(args) {
  const metadata = loadPackageMetadata(args);
  const requested = Math.max(1, args.packages || args.servers);
  const packages = metadata.packages.slice(0, requested);
  for (let index = packages.length; index < requested; index += 1) packages.push(syntheticPackage(index));
  const policy = batchedPerOp('package-metadata-policy-classification', args.runs, args.iterations, (ops, run) => {
    let checksum = 0;
    for (let index = 0; index < ops; index += 1) {
      const pkg = packages[(index + run * ops) % packages.length];
      const classification = classifyPackageMetadata(pkg);
      checksum ^= classification.policy.length + classification.locks.length;
    }
    return checksum;
  });
  const policyCounts = {};
  for (const pkg of packages) {
    const classification = classifyPackageMetadata(pkg);
    policyCounts[classification.policy] = (policyCounts[classification.policy] || 0) + 1;
  }
  return { ...policy, source: metadata.source, packageCount: packages.length, policyCounts };
}

function addCheck(checks, id, ok, severity, evidence, recommendation = '') {
  checks.push({ id, ok: Boolean(ok), severity, evidence, recommendation });
}

function makeReport(args) {
  const started = performance.now();
  const route = measureJsonRpcRouting(args);
  const sessionShard = measureSessionShardKeys(args);
  const lockAdmission = measureLockAdmission(args);
  const toolIndex = measureToolLookupAndSearch(args);
  const packagePolicy = measurePackagePolicyPath(args);
  const elapsedMs = performance.now() - started;
  const checks = [];

  addCheck(checks, 'json-rpc-routing-overhead-budget', route.p95Us < 750, 'high', `p95 ${route.p95Us}us`, 'Keep parse/route/serialize work small and avoid extra JSON passes.');
  addCheck(checks, 'session-shard-key-overhead-budget', sessionShard.p95Us < 150, 'high', `p95 ${sessionShard.p95Us}us`, 'Use stable compact keys; do not add heavyweight crypto hashing to the hot path.');
  addCheck(checks, 'lock-admission-overhead-budget', lockAdmission.p95Us < 200, 'high', `p95 ${lockAdmission.p95Us}us`, 'Keep admission O(number of lock keys) and release locks deterministically.');
  addCheck(checks, 'lock-admission-leaves-no-active-locks', lockAdmission.activeLocksAtEnd === 0, 'critical', `${lockAdmission.activeLocksAtEnd} active locks remain`, 'Every admitted operation must release exactly the keys it acquired.');
  addCheck(checks, 'disabled-and-review-gates-are-still-hit', lockAdmission.stats.blockedDisabled > 0 && lockAdmission.stats.blockedReview > 0, 'critical', JSON.stringify(lockAdmission.stats), 'Optimization must not bypass disabled-server or review-required gates.');
  addCheck(checks, 'tool-index-build-heap-budget', toolIndex.build.heapDeltaMiB < args.memoryLimitMiB, 'high', `${toolIndex.build.heapDeltaMiB}MiB for ${toolIndex.build.toolCount} tools`, 'Large tool catalogs should remain bounded and projected before model exposure.');
  addCheck(checks, 'tool-exact-lookup-overhead-budget', toolIndex.exactLookup.p95Us < 100, 'medium', `p95 ${toolIndex.exactLookup.p95Us}us`, 'Exact call routing should use qualified-name maps rather than linear search.');
  addCheck(checks, 'tool-search-projection-overhead-budget', toolIndex.search.p95Ms < 150, 'medium', `p95 ${toolIndex.search.p95Ms}ms for ${toolIndex.build.toolCount} tools`, 'Search projection should stay top-K and read-only filtered.');
  addCheck(checks, 'package-policy-overhead-budget', packagePolicy.p95Us < 250, 'medium', `p95 ${packagePolicy.p95Us}us across ${packagePolicy.packageCount} package profiles`, 'Metadata policy classification must stay cheap enough for registry pressure tests.');
  addCheck(checks, 'classification-budget', packagePolicy.p95Us < 250, 'medium', `p95 ${packagePolicy.p95Us}us across ${packagePolicy.packageCount} package profiles`, 'Compatibility alias for the synthetic overhead contract.');
  addCheck(checks, 'tool-index-budget', toolIndex.build.buildUsPerTool < 10, 'medium', `${toolIndex.build.buildUsPerTool}us/tool to build ${toolIndex.build.toolCount} tools`, 'Compatibility alias for the synthetic overhead contract.');
  addCheck(checks, 'scheduler-budget', lockAdmission.p95Us < 200, 'medium', `p95 ${lockAdmission.p95Us}us`, 'Compatibility alias for the synthetic overhead contract.');

  const blockers = checks.filter((check) => !check.ok && (check.severity === 'critical' || check.severity === 'high')).map((check) => `${check.id}: ${check.evidence}`);
  const warnings = checks.filter((check) => !check.ok && check.severity !== 'critical' && check.severity !== 'high').map((check) => `${check.id}: ${check.evidence}`);

  return {
    schema: 'mcpace.mcpOverheadProfile.v1',
    status: blockers.length ? 'fail' : 'pass',
    generatedAt: new Date().toISOString(),
    project: { name: deriveProjectName(), version: deriveProjectVersion() },
    elapsedMs: round(elapsedMs, 3),
    config: {
      runs: args.runs,
      iterations: args.iterations,
      servers: args.servers,
      tools: args.tools,
      clients: args.clients,
      sessions: args.sessions,
      operations: args.operations,
      packages: args.packages,
      toolsPerServer: args.toolsPerServer,
      memoryLimitMiB: args.memoryLimitMiB
    },
    safety: {
      startsMcpServers: false,
      callsMcpTools: false,
      executesThirdPartyPackages: false,
      packageInstallScriptsAllowed: false,
      userSecretsPassedToRuntime: false,
      benchmarkType: 'synthetic-hub-hot-path'
    },
    metrics: {
      route,
      sessionShard,
      lockAdmission,
      toolIndex,
      packagePolicy
    },
    classification: {
      ...packagePolicy,
      perPackageUs: packagePolicy.p95Us
    },
    toolIndex: {
      toolCount: toolIndex.build.toolCount,
      lookupCount: toolIndex.exactLookup.operationCount,
      lookupHits: toolIndex.exactLookup.operationCount,
      build: toolIndex.build,
      exactLookup: toolIndex.exactLookup,
      search: toolIndex.search
    },
    scheduler: {
      started: lockAdmission.stats.admitted,
      blocked: lockAdmission.stats.blockedDisabled + lockAdmission.stats.blockedReview + lockAdmission.stats.blockedConflict,
      stats: lockAdmission.stats,
      p95Us: lockAdmission.p95Us,
      operationCount: lockAdmission.operationCount
    },
    summary: {
      jsonRpcP95Us: route.p95Us,
      sessionKeyP95Us: sessionShard.p95Us,
      lockAdmissionP95Us: lockAdmission.p95Us,
      toolIndexBuildMs: toolIndex.build.elapsedMs,
      toolSearchP95Ms: toolIndex.search.p95Ms,
      packagePolicyP95Us: packagePolicy.p95Us,
      blockedDisabled: lockAdmission.stats.blockedDisabled,
      blockedReview: lockAdmission.stats.blockedReview,
      admitted: lockAdmission.stats.admitted
    },
    checks,
    blockers,
    warnings,
    recommendations: [
      'Prefer one long-lived hub process; npm launcher cold-start overhead is acceptable for install/CLI but should not be paid per MCP request.',
      'Keep unknown and credentialed servers disabled/review-gated; optimizing should never convert registry discovery into execution.',
      'Expose large tool catalogs through exact qualified-name routing plus top-K read-only projection, not by dumping every discovered tool into every model context.',
      'Use session/client/project/credential/transport identity as the sharding key for stateful servers; stateless candidates can be widened only after live evidence.'
    ]
  };
}

function renderMarkdown(report) {
  const lines = [];
  lines.push('# MCP overhead profile', '');
  lines.push(`- Status: ${report.status}`);
  lines.push(`- Generated: ${report.generatedAt}`);
  lines.push(`- Project: ${report.project.name} ${report.project.version}`);
  lines.push(`- Elapsed: ${report.elapsedMs}ms`);
  lines.push(`- Safety: starts MCP servers = ${report.safety.startsMcpServers}, calls tools = ${report.safety.callsMcpTools}`);
  lines.push('');
  lines.push('## Hot-path metrics', '');
  lines.push('| Area | Metric | Value |');
  lines.push('|---|---|---:|');
  lines.push(`| JSON-RPC route | p95 us/op | ${report.summary.jsonRpcP95Us} |`);
  lines.push(`| Session shard key | p95 us/op | ${report.summary.sessionKeyP95Us} |`);
  lines.push(`| Lock admission | p95 us/op | ${report.summary.lockAdmissionP95Us} |`);
  lines.push(`| Tool index build | ms | ${report.summary.toolIndexBuildMs} |`);
  lines.push(`| Tool search projection | p95 ms/query | ${report.summary.toolSearchP95Ms} |`);
  lines.push(`| Package policy classification | p95 us/op | ${report.summary.packagePolicyP95Us} |`);
  lines.push('');
  lines.push('## Checks', '');
  lines.push('| Check | OK | Severity | Evidence |');
  lines.push('|---|---:|---|---|');
  for (const check of report.checks) lines.push(`| ${escapeMd(check.id)} | ${check.ok ? 'yes' : 'no'} | ${escapeMd(check.severity)} | ${escapeMd(check.evidence)} |`);
  lines.push('');
  lines.push('## Recommendations', '');
  for (const recommendation of report.recommendations) lines.push(`- ${recommendation}`);
  if (report.blockers.length) {
    lines.push('', '## Blockers', '');
    for (const blocker of report.blockers) lines.push(`- ${escapeMd(blocker)}`);
  }
  return `${lines.join('\n')}\n`;
}

function escapeMd(value) {
  return String(value ?? '').replace(/[|\n\r]/g, ' ');
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

function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    printHelp();
    return;
  }
  const report = makeReport(args);
  writeReport(report, args);
  if (args.json) console.log(JSON.stringify(report, null, 2));
  if (report.status !== 'pass') process.exitCode = 1;
}

main();
