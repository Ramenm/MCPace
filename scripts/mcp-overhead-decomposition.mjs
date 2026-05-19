#!/usr/bin/env node
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { performance } from 'node:perf_hooks';
import { repoRoot, deriveProjectName, deriveProjectVersion } from './lib/project-metadata.mjs';
import { classifyPackageMetadata } from './lib/mcp-package-policy.mjs';

const DEFAULTS = {
  servers: 100,
  toolsPerServer: 50,
  lookups: 20_000,
  jsonIterations: 20_000,
  projectionIterations: 250,
  schedulerOperations: 50_000,
  memoryLimitMiB: 256,
  write: 'reports/mcp-overhead-decomposition-latest.json',
  markdown: 'reports/mcp-overhead-decomposition-latest.md',
};

let sink = 0;

function parseArgs(argv) {
  const args = { ...DEFAULTS, json: false, noWrite: false, help: false };
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
      case '--servers': args.servers = positiveInteger(readValue(), token, 1, 1000); break;
      case '--tools-per-server': args.toolsPerServer = positiveInteger(readValue(), token, 1, 5000); break;
      case '--lookups': args.lookups = positiveInteger(readValue(), token, 1, 1_000_000); break;
      case '--json-iterations': args.jsonIterations = positiveInteger(readValue(), token, 1, 1_000_000); break;
      case '--projection-iterations': args.projectionIterations = positiveInteger(readValue(), token, 1, 100_000); break;
      case '--scheduler-operations': args.schedulerOperations = positiveInteger(readValue(), token, 1, 5_000_000); break;
      case '--memory-limit-mib': args.memoryLimitMiB = positiveInteger(readValue(), token, 1, 4096); break;
      case '--write': args.write = readValue(); break;
      case '--markdown': args.markdown = readValue(); break;
      case '--no-write': args.noWrite = true; args.write = null; args.markdown = null; break;
      case '--help':
      case '-h': args.help = true; break;
      default: throw new Error(`unsupported mcp-overhead-decomposition argument: ${token}`);
    }
  }
  return args;
}

function positiveInteger(value, label, min, max) {
  const parsed = Number.parseInt(value, 10);
  if (!Number.isSafeInteger(parsed) || parsed < min || parsed > max) throw new Error(`${label} must be an integer between ${min} and ${max}`);
  return parsed;
}

function help() {
  console.log(`Usage: node scripts/mcp-overhead-decomposition.mjs [--json]\n\nMeasures bounded MCP-hub overhead without starting third-party MCP servers:\n  - JSON-RPC parse/stringify cost;\n  - tool route lookup: linear scan vs prebuilt Map index;\n  - visibility projection: uncached scan vs cached projection;\n  - scheduler lock acquire/release cost;\n  - shared metadata policy classifier cost with fingerprint cache.\n\nOptions:\n  --servers 100\n  --tools-per-server 50\n  --lookups 20000\n  --json-iterations 20000\n  --projection-iterations 250\n  --scheduler-operations 50000\n  --memory-limit-mib 256\n  --write reports/mcp-overhead-decomposition-latest.json\n  --markdown reports/mcp-overhead-decomposition-latest.md\n  --no-write\n`);
}

function now() { return performance.now(); }
function ms(value) { return Number(value.toFixed(4)); }
function mib(bytes) { return Number((bytes / 1024 / 1024).toFixed(4)); }
function ratio(slow, fast) { return fast > 0 ? Number((slow / fast).toFixed(2)) : null; }

function timed(id, iterations, fn) {
  fn(Math.min(10, iterations));
  const started = now();
  const result = fn(iterations);
  const elapsedMs = now() - started;
  return { id, iterations, elapsedMs: ms(elapsedMs), perOperationMs: ms(elapsedMs / Math.max(iterations, 1)), result };
}

function normalize(value) {
  return String(value || '').toLowerCase().replace(/[^a-z0-9_.-]+/g, ' ').trim();
}

function syntheticInventory(serverCount, toolsPerServer) {
  const tools = [];
  for (let serverIndex = 0; serverIndex < serverCount; serverIndex += 1) {
    const serverId = `srv-${String(serverIndex).padStart(3, '0')}`;
    const serverPolicy = serverIndex % 11 === 0 ? 'credential-review' : serverIndex % 7 === 0 ? 'project-single-writer' : 'readonly-candidate';
    for (let toolIndex = 0; toolIndex < toolsPerServer; toolIndex += 1) {
      const readOnly = toolIndex % 10 < 7;
      const family = readOnly ? ['read', 'search', 'list', 'describe'][toolIndex % 4] : ['write', 'delete', 'update'][toolIndex % 3];
      const name = `${family}_${toolIndex}`;
      const qualifiedName = `${serverId}.${name}`;
      const description = `${readOnly ? 'Read-only' : 'Mutating'} ${family} tool ${toolIndex} on ${serverId}`;
      const normalized = normalize(`${serverId} ${name} ${qualifiedName} ${description}`);
      tools.push({ serverId, name, qualifiedName, description, readOnly, mutating: !readOnly, policy: serverPolicy, normalized });
    }
  }
  return tools;
}

function buildRouteIndex(tools) {
  const byQualifiedName = new Map();
  const byServerAndName = new Map();
  for (const tool of tools) {
    byQualifiedName.set(tool.qualifiedName, tool);
    byServerAndName.set(`${tool.serverId}\0${tool.name}`, tool);
  }
  return { byQualifiedName, byServerAndName };
}

function linearLookup(tools, query) {
  for (const tool of tools) {
    if (tool.qualifiedName === query) return tool;
  }
  return null;
}

function indexedLookup(index, query) {
  return index.byQualifiedName.get(query) || null;
}

function makeLookupQueries(tools, count) {
  const queries = [];
  for (let index = 0; index < count; index += 1) {
    const tool = tools[(index * 7919) % tools.length];
    queries.push(index % 19 === 0 ? `missing.${index}` : tool.qualifiedName);
  }
  return queries;
}

function score(tool, terms) {
  let total = 0;
  for (const term of terms) {
    if (tool.normalized.includes(term)) total += term.length;
    if (tool.qualifiedName.includes(term)) total += 10;
    if (tool.name.includes(term)) total += 15;
  }
  return total;
}

function naiveProjection(tools, query, limit) {
  const terms = normalize(query).split(/\s+/).filter(Boolean);
  const scored = [];
  for (const raw of tools) {
    if (raw.mutating || raw.policy.includes('credential')) continue;
    const normalized = normalize(`${raw.serverId} ${raw.name} ${raw.qualifiedName} ${raw.description}`);
    let value = 0;
    for (const term of terms) {
      if (normalized.includes(term)) value += term.length;
      if (raw.qualifiedName.includes(term)) value += 10;
      if (raw.name.includes(term)) value += 15;
    }
    if (value > 0) scored.push({ tool: raw, score: value });
  }
  scored.sort((left, right) => right.score - left.score || left.tool.qualifiedName.localeCompare(right.tool.qualifiedName));
  return scored.slice(0, limit).map((item) => item.tool.qualifiedName);
}

function insertTopK(items, item, limit) {
  let position = items.length;
  while (position > 0) {
    const previous = items[position - 1];
    if (previous.score > item.score) break;
    if (previous.score === item.score && previous.key.localeCompare(item.key) <= 0) break;
    position -= 1;
  }
  if (position >= limit) return;
  items.splice(position, 0, item);
  if (items.length > limit) items.length = limit;
}

function optimizedProjection(tools, query, limit, cache, scope) {
  const cacheKey = `${scope}\0${query}\0${limit}`;
  const cached = cache.get(cacheKey);
  if (cached) return cached;
  const terms = normalize(query).split(/\s+/).filter(Boolean);
  const top = [];
  for (const tool of tools) {
    if (tool.mutating || tool.policy.includes('credential')) continue;
    const value = score(tool, terms);
    if (value > 0) insertTopK(top, { key: tool.qualifiedName, score: value }, limit);
  }
  const projected = top.map((item) => item.key);
  cache.set(cacheKey, projected);
  return projected;
}

function jsonRpcBenchmarks(args, tools) {
  const initializeMessage = {
    jsonrpc: '2.0',
    id: 1,
    method: 'initialize',
    params: { protocolVersion: '2025-11-25', capabilities: { roots: { listChanged: true } }, clientInfo: { name: 'mcpace-overhead-benchmark', version: '0.0.0' } },
  };
  const toolListMessage = {
    jsonrpc: '2.0',
    id: 2,
    result: {
      tools: tools.slice(0, Math.min(1000, tools.length)).map((tool) => ({
        name: tool.qualifiedName,
        title: tool.name,
        description: tool.description,
        annotations: { readOnlyHint: tool.readOnly, destructiveHint: tool.mutating },
        inputSchema: { type: 'object', properties: { value: { type: 'string' } }, additionalProperties: false },
      })),
    },
  };
  const small = timed('json-rpc-small-roundtrip', args.jsonIterations, (n) => {
    let local = 0;
    for (let index = 0; index < n; index += 1) {
      const parsed = JSON.parse(JSON.stringify(initializeMessage));
      local += parsed.id;
    }
    sink += local;
    return { checksum: local };
  });
  const largeIterations = Math.max(10, Math.floor(args.jsonIterations / 40));
  const large = timed('json-rpc-tools-list-roundtrip', largeIterations, (n) => {
    let local = 0;
    for (let index = 0; index < n; index += 1) {
      const parsed = JSON.parse(JSON.stringify(toolListMessage));
      local += parsed.result.tools.length;
    }
    sink += local;
    return { checksum: local, toolDescriptors: toolListMessage.result.tools.length };
  });
  return { small, large };
}

function routeBenchmarks(args, tools, routeIndex) {
  const queries = makeLookupQueries(tools, args.lookups);
  const linear = timed('route-linear-scan', queries.length, (n) => {
    let found = 0;
    for (let index = 0; index < n; index += 1) if (linearLookup(tools, queries[index])) found += 1;
    sink += found;
    return { found };
  });
  const indexed = timed('route-indexed-map', queries.length, (n) => {
    let found = 0;
    for (let index = 0; index < n; index += 1) if (indexedLookup(routeIndex, queries[index])) found += 1;
    sink += found;
    return { found };
  });
  return { linear, indexed, speedup: ratio(linear.elapsedMs, indexed.elapsedMs) };
}

function projectionBenchmarks(args, tools) {
  const query = 'read search list docs';
  const limit = 32;
  const cache = new Map();
  const scope = 'client:claude/session:chat-a/project:/work/a';
  const uncached = timed('visibility-projection-uncached', args.projectionIterations, (n) => {
    let local = 0;
    for (let index = 0; index < n; index += 1) local += naiveProjection(tools, query, limit).length;
    sink += local;
    return { projected: local };
  });
  cache.clear();
  const optimizedMiss = timed('visibility-projection-optimized-cache-miss', args.projectionIterations, (n) => {
    let local = 0;
    for (let index = 0; index < n; index += 1) {
      cache.clear();
      local += optimizedProjection(tools, query, limit, cache, `${scope}/miss-${index}`).length;
    }
    sink += local;
    return { projected: local };
  });
  cache.clear();
  optimizedProjection(tools, query, limit, cache, scope);
  const cached = timed('visibility-projection-cache-hit', args.projectionIterations, (n) => {
    let local = 0;
    for (let index = 0; index < n; index += 1) local += optimizedProjection(tools, query, limit, cache, scope).length;
    sink += local;
    return { projected: local, cacheSize: cache.size };
  });
  return {
    uncached,
    optimizedMiss,
    cached,
    optimizedMissSpeedup: ratio(uncached.elapsedMs, optimizedMiss.elapsedMs),
    cacheHitSpeedup: ratio(uncached.elapsedMs, cached.elapsedMs),
  };
}

function schedulerBenchmark(args) {
  const clients = ['claude', 'cursor', 'vscode', 'codex'];
  const sessions = ['chat-a', 'chat-b', 'chat-c', 'chat-d'];
  const projects = ['/work/a', '/work/b', '/work/c'];
  const profiles = [
    { id: 'filesystem', max: 1, locks: (i) => [`project:${projects[i % projects.length]}`, `file:${projects[i % projects.length]}`] },
    { id: 'memory', max: 1, locks: (i) => [`session:${sessions[i % sessions.length]}`, `context:${sessions[i % sessions.length]}:${i % 3}`] },
    { id: 'remote-stateless', max: 4, locks: (i) => [`provider:${i % 5}`] },
    { id: 'credential-api', max: 1, locks: (i) => [`credential:user-${i % 3}`, `tenant:t-${i % 2}`] },
  ];
  const activeLocks = new Set();
  const activeWorkers = new Map();
  const pendingReleases = [];
  const result = timed('scheduler-lock-cycle', args.schedulerOperations, (n) => {
    let started = 0;
    let blocked = 0;
    for (let index = 0; index < n; index += 1) {
      while (pendingReleases.length && pendingReleases[0].at <= index) {
        const release = pendingReleases.shift();
        for (const lock of release.locks) activeLocks.delete(lock);
        activeWorkers.set(release.worker, Math.max(0, (activeWorkers.get(release.worker) || 0) - 1));
      }
      const profile = profiles[index % profiles.length];
      const client = clients[index % clients.length];
      const locks = profile.locks(index);
      const worker = `${profile.id}:${client}:${locks[0]}`;
      const workerCount = activeWorkers.get(worker) || 0;
      const conflict = locks.some((lock) => activeLocks.has(lock));
      if (conflict || workerCount >= profile.max) {
        blocked += 1;
        continue;
      }
      for (const lock of locks) activeLocks.add(lock);
      activeWorkers.set(worker, workerCount + 1);
      pendingReleases.push({ at: index + 2 + (index % 5), locks, worker });
      started += 1;
    }
    while (pendingReleases.length) {
      const release = pendingReleases.shift();
      for (const lock of release.locks) activeLocks.delete(lock);
      activeWorkers.set(release.worker, Math.max(0, (activeWorkers.get(release.worker) || 0) - 1));
    }
    sink += started + blocked;
    return { started, blocked, remainingLocks: activeLocks.size };
  });
  return result;
}

function loadPackageSamples() {
  const candidates = [
    'reports/mcp-mass-package-survey-latest.json',
    'reports/mcp-mass-package-survey-fixture-latest.json',
    'eval/fixtures/mcp-mass-package-survey-sample.json',
  ];
  for (const rel of candidates) {
    const full = path.join(repoRoot, rel);
    if (!fs.existsSync(full)) continue;
    try {
      const parsed = JSON.parse(fs.readFileSync(full, 'utf8'));
      if (Array.isArray(parsed.packages) && parsed.packages.length) return { source: rel, packages: parsed.packages };
    } catch { /* ignore malformed historical reports */ }
  }
  return { source: 'synthetic-fallback', packages: [] };
}

function classifyPackage(pkg) {
  const classification = classifyPackageMetadata(pkg);
  return {
    signals: classification.signals,
    state: classification.stateClass,
    policy: classification.policy,
  };
}

function packageFingerprint(pkg) {
  const keywords = Array.isArray(pkg.keywords) ? pkg.keywords.join(',') : '';
  return `${pkg.name || ''}\0${pkg.version || ''}\0${pkg.description || ''}\0${keywords}`;
}

function classifierBenchmark(args) {
  const samples = loadPackageSamples();
  const packages = samples.packages.length ? samples.packages : Array.from({ length: 100 }, (_, index) => ({ name: `synthetic-mcp-${index}`, description: index % 3 === 0 ? 'MCP filesystem server' : index % 3 === 1 ? 'MCP API search server with OAuth' : 'MCP memory context server', keywords: ['mcp'] }));
  const iterations = Math.max(1, Math.floor(args.lookups / 2));
  const cache = new Map();
  const measured = timed('metadata-signal-classifier-cached', iterations, (n) => {
    let credential = 0;
    let hits = 0;
    let misses = 0;
    for (let index = 0; index < n; index += 1) {
      const pkg = packages[index % packages.length];
      const key = packageFingerprint(pkg);
      let result = cache.get(key);
      if (result) {
        hits += 1;
      } else {
        result = classifyPackage(pkg);
        cache.set(key, result);
        misses += 1;
      }
      if (/credential|admin|sensitive/.test(result.policy)) credential += 1;
    }
    sink += credential + hits + misses;
    return { classified: n, credential, cacheSize: cache.size, hits, misses };
  });
  return { source: samples.source, packageSamples: packages.length, measured, cacheSize: cache.size };
}

function check(id, ok, detail) {
  return { id, ok: Boolean(ok), status: ok ? 'pass' : 'fail', detail };
}

function makeReport(args) {
  const startedAt = now();
  const heapStart = process.memoryUsage().heapUsed;
  const tools = syntheticInventory(args.servers, args.toolsPerServer);
  const indexBuild = timed('route-index-build', 1, () => buildRouteIndex(tools));
  const routeIndex = indexBuild.result;
  const jsonRpc = jsonRpcBenchmarks(args, tools);
  const routing = routeBenchmarks(args, tools, routeIndex);
  const projection = projectionBenchmarks(args, tools);
  const scheduler = schedulerBenchmark(args);
  const classifier = classifierBenchmark(args);
  const heapDeltaMiB = mib(process.memoryUsage().heapUsed - heapStart);
  const elapsedMs = ms(now() - startedAt);
  const checks = [
    check('does-not-start-random-mcp-servers', true, 'Synthetic benchmark only; no package bins, MCP processes, or tool invocations are executed.'),
    check('route-index-is-faster-than-linear-scan', routing.speedup !== null && routing.speedup >= 5, `speedup=${routing.speedup}x`),
    check('route-index-build-is-bounded', indexBuild.elapsedMs < 250, `index build ${indexBuild.elapsedMs}ms for ${tools.length} tools`),
    check('visibility-cache-hit-is-faster-than-uncached', projection.cacheHitSpeedup !== null && projection.cacheHitSpeedup >= 20, `speedup=${projection.cacheHitSpeedup}x`),
    check('visibility-optimized-miss-is-not-slower-than-naive', projection.optimizedMiss.elapsedMs <= projection.uncached.elapsedMs * 1.1, `optimizedMiss=${projection.optimizedMiss.elapsedMs}ms; naive=${projection.uncached.elapsedMs}ms`),
    check('scheduler-lock-overhead-is-bounded', scheduler.perOperationMs < 0.01, `per operation ${scheduler.perOperationMs}ms`),
    check('scheduler-drains-all-synthetic-locks', scheduler.result.remainingLocks === 0, `remaining locks ${scheduler.result.remainingLocks}`),
    check('small-json-rpc-overhead-is-bounded', jsonRpc.small.perOperationMs < 0.02, `per roundtrip ${jsonRpc.small.perOperationMs}ms`),
    check('large-tools-list-json-rpc-is-measured', jsonRpc.large.result.toolDescriptors > 0 && jsonRpc.large.perOperationMs > 0, `${jsonRpc.large.result.toolDescriptors} descriptors; ${jsonRpc.large.perOperationMs}ms/roundtrip`),
    check('metadata-classifier-overhead-is-bounded', classifier.measured.perOperationMs < 0.25, `per package ${classifier.measured.perOperationMs}ms`),
    check('heap-growth-under-budget', heapDeltaMiB <= args.memoryLimitMiB, `heap delta ${heapDeltaMiB}MiB; budget ${args.memoryLimitMiB}MiB`),
  ];
  const blockers = checks.filter((item) => !item.ok).map((item) => `${item.id}: ${item.detail}`);
  return {
    schema: 'mcpace.mcpOverheadDecomposition.v1',
    generatedAt: new Date().toISOString(),
    status: blockers.length ? 'fail' : 'pass',
    project: { name: deriveProjectName(), version: deriveProjectVersion() },
    environment: { node: process.version, platform: process.platform, arch: process.arch, cpuCount: os.cpus().length, availableParallelism: os.availableParallelism?.() || os.cpus().length || null },
    scenario: { servers: args.servers, toolsPerServer: args.toolsPerServer, toolCount: tools.length, lookups: args.lookups, jsonIterations: args.jsonIterations, projectionIterations: args.projectionIterations, schedulerOperations: args.schedulerOperations, memoryLimitMiB: args.memoryLimitMiB },
    safety: { startsMcpServers: false, callsMcpTools: false, executesThirdPartyPackages: false, packageInstallScriptsAllowed: false, usesSyntheticInventory: true },
    summary: {
      elapsedMs,
      heapDeltaMiB,
      routeIndexSpeedup: routing.speedup,
      projectionCacheHitSpeedup: projection.cacheHitSpeedup,
      projectionOptimizedMissSpeedup: projection.optimizedMissSpeedup,
      schedulerPerOperationMs: scheduler.perOperationMs,
      smallJsonRpcPerOperationMs: jsonRpc.small.perOperationMs,
      largeToolsListPerOperationMs: jsonRpc.large.perOperationMs,
      metadataClassifierPerOperationMs: classifier.measured.perOperationMs,
      sink,
    },
    measurements: { indexBuild, jsonRpc, routing, projection, scheduler, classifier },
    checks,
    blockers,
  };
}

function markdown(report) {
  const lines = [
    '# MCP overhead decomposition',
    '',
    `Generated: ${report.generatedAt}`,
    `Status: **${report.status}**`,
    '',
    '## Scenario',
    '',
    `- Servers: ${report.scenario.servers}`,
    `- Tools per server: ${report.scenario.toolsPerServer}`,
    `- Total synthetic tools: ${report.scenario.toolCount}`,
    `- Lookups: ${report.scenario.lookups}`,
    `- Scheduler operations: ${report.scenario.schedulerOperations}`,
    '',
    '## Summary',
    '',
    `- Route index speedup over linear scan: ${report.summary.routeIndexSpeedup}x`,
    `- Visibility cache-hit speedup over uncached projection: ${report.summary.projectionCacheHitSpeedup}x`,
    `- Scheduler lock cycle: ${report.summary.schedulerPerOperationMs} ms/op`,
    `- Small JSON-RPC roundtrip: ${report.summary.smallJsonRpcPerOperationMs} ms/op`,
    `- 1k tools/list JSON-RPC roundtrip: ${report.summary.largeToolsListPerOperationMs} ms/op`,
    `- Metadata classifier: ${report.summary.metadataClassifierPerOperationMs} ms/op`,
    `- Heap delta: ${report.summary.heapDeltaMiB} MiB`,
    '',
    '## Optimizations locked by this benchmark',
    '',
    '- Keep a prebuilt `qualifiedToolName -> route` index instead of scanning all tools per call.',
    '- Cache client/session/project visibility projections and invalidate on server/tool/config change.',
    '- Keep scheduler lock acquisition O(number-of-locks) and never proportional to total installed servers.',
    '- Measure JSON-RPC payload overhead separately from process spawn and HTTP connection setup.',
    '- Cache package metadata classification by normalized package fingerprint so registry/UI refreshes do not re-run signal inference.',
    '- Do not run random MCP servers during ecosystem surveys; benchmark synthetic inventory or reviewed safe probes only.',
    '',
    '## Checks',
    '',
    '| Check | Status | Detail |',
    '|---|---:|---|',
  ];
  for (const check of report.checks) lines.push(`| ${check.id} | ${check.ok ? 'pass' : 'fail'} | ${String(check.detail).replace(/\|/g, '\\|')} |`);
  if (report.blockers.length) {
    lines.push('', '## Blockers', '');
    for (const blocker of report.blockers) lines.push(`- ${blocker}`);
  }
  return `${lines.join('\n')}\n`;
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
  if (args.help) {
    help();
    return;
  }
  const report = makeReport(args);
  writeReport(report, args);
  if (args.json) console.log(JSON.stringify(report, null, 2));
  else process.stdout.write(markdown(report));
  if (report.status !== 'pass') process.exitCode = 1;
}

main();
