#!/usr/bin/env node
import { spawn } from 'node:child_process';
import { existsSync, statSync } from 'node:fs';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import http from 'node:http';
import net from 'node:net';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { performance } from 'node:perf_hooks';
import { repoRoot } from './lib/project-metadata.mjs';
import { cleanChildEnv } from './lib/safe-child-env.mjs';

function parseArgs(argv) {
  const parsed = {
    binary: explicitBinaryFromEnv(),
    root: '',
    durationMs: 10_000,
    concurrency: 50,
    port: 0,
    maxConnections: 256,
    maxBodyBytes: 65_536,
    overviewCacheMs: 250,
    json: false,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    const readValue = () => {
      const value = argv[index + 1];
      if (!value) throw new Error(`${arg} requires a value`);
      index += 1;
      return value;
    };
    switch (arg) {
      case '--binary':
        parsed.binary = readValue();
        break;
      case '--root':
        parsed.root = readValue();
        break;
      case '--port':
        parsed.port = positiveInteger(readValue(), arg);
        break;
      case '--duration-ms':
        parsed.durationMs = positiveInteger(readValue(), arg);
        break;
      case '--concurrency':
        parsed.concurrency = positiveInteger(readValue(), arg);
        break;
      case '--max-connections':
        parsed.maxConnections = positiveInteger(readValue(), arg);
        break;
      case '--max-body-bytes':
        parsed.maxBodyBytes = positiveInteger(readValue(), arg);
        break;
      case '--overview-cache-ms':
        parsed.overviewCacheMs = nonnegativeInteger(readValue(), arg);
        break;
      case '--json':
        parsed.json = true;
        break;
      case '-h':
      case '--help':
        printHelp();
        process.exit(0);
        break;
      default:
        throw new Error(`unknown argument: ${arg}`);
    }
  }
  parsed.binary ||= defaultBinary();
  return parsed;
}

function positiveInteger(value, name) {
  const number = Number(value);
  if (!Number.isSafeInteger(number) || number <= 0) {
    throw new Error(`${name} must be a positive integer`);
  }
  return number;
}

function nonnegativeInteger(value, name) {
  const number = Number(value);
  if (!Number.isSafeInteger(number) || number < 0) {
    throw new Error(`${name} must be a non-negative integer`);
  }
  return number;
}

function unquotePath(value) {
  const trimmed = String(value || '').trim();
  if (trimmed.length >= 2) {
    const first = trimmed[0];
    const last = trimmed[trimmed.length - 1];
    if ((first === '"' && last === '"') || (first === "'" && last === "'")) {
      return trimmed.slice(1, -1);
    }
  }
  return trimmed;
}

function explicitBinaryFromEnv() {
  for (const name of ['MCPACE_BINARY', 'MCPACE_BINARY_PATH', 'MCPACE_DEV_BINARY']) {
    const value = unquotePath(process.env[name]);
    if (value) return value;
  }
  return '';
}

function binaryName() {
  return process.platform === 'win32' ? 'mcpace.exe' : 'mcpace';
}

function defaultBinaryCandidates() {
  return [
    path.join(repoRoot, 'target', 'release', binaryName()),
    path.join(repoRoot, 'target', 'debug', binaryName()),
  ];
}

function defaultBinary() {
  const candidates = defaultBinaryCandidates();
  return candidates.find((candidate) => existsSync(candidate)) || candidates[0];
}

function assertRunnableBinary(binaryPath) {
  let stat;
  try {
    stat = statSync(binaryPath);
  } catch {
    const defaults = defaultBinaryCandidates().map((candidate) => path.relative(repoRoot, candidate)).join(', ');
    throw new Error(
      `MCPace binary not found: ${binaryPath}. Build one with cargo build --release, pass --binary <path>, or set MCPACE_BINARY_PATH/MCPACE_DEV_BINARY. Checked defaults: ${defaults}.`
    );
  }
  if (!stat.isFile()) {
    throw new Error(`MCPace binary path is not a file: ${binaryPath}`);
  }
  if (process.platform !== 'win32' && (stat.mode & 0o111) === 0) {
    throw new Error(`MCPace binary path is not executable: ${binaryPath}`);
  }
}

function printHelp() {
  console.log(`Usage: node scripts/load-test-local.mjs [options]\n\nOptions:\n  --binary <path>           MCPace binary to run; env fallback: MCPACE_BINARY, MCPACE_BINARY_PATH, MCPACE_DEV_BINARY; default: target/release/mcpace, then target/debug/mcpace\n  --root <path>             Existing MCPace root. Omit to create an isolated temporary root\n  --port <n>                Server port. Default: auto-reserve a free loopback port
  --duration-ms <n>         Duration per load scenario. Default: 10000\n  --concurrency <n>         Concurrent request loops per scenario. Default: 50\n  --max-connections <n>     Server-side connection cap. Default: 256\n  --max-body-bytes <n>      Server-side body cap, also used by edge-case probes. Default: 65536\n  --overview-cache-ms <n>   Server overview cache TTL. Default: 250\n  --json                   Emit JSON only`);
}

async function makeIsolatedRoot() {
  const root = await mkdtemp(path.join(tmpdir(), 'mcpace-load-'));
  await mkdir(path.join(root, 'mcp_settings.d'), { recursive: true });
  await writeFile(
    path.join(root, 'mcpace.config.json'),
    `${JSON.stringify({
      name: 'mcpace-load-test',
      version: '0.6.9',
      profiles: {
        runtime: {
          default: 'manual',
          profiles: {
            manual: { description: 'load-test empty profile', serverOverrides: {} },
          },
        },
      },
      serve: { host: '127.0.0.1', port: 0, mcpPath: '/mcp', publicUrl: '' },
      mcpSettings: { includeDirs: ['mcp_settings.d'], includePaths: [] },
    }, null, 2)}\n`,
  );
  await writeFile(path.join(root, 'mcp_settings.json'), '{"mcpServers":{}}\n');
  return root;
}

async function reserveLoopbackPort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const address = server.address();
      const port = typeof address === 'object' && address ? address.port : 0;
      server.close(() => resolve(port));
    });
  });
}

async function startServer(options) {
  assertRunnableBinary(options.binary);
  const root = options.root || (await makeIsolatedRoot());
  const selectedPort = options.port || (await reserveLoopbackPort());
  const child = spawn(
    options.binary,
    [
      'serve',
      '--root',
      root,
      '--host',
      '127.0.0.1',
      '--port',
      String(selectedPort),
      '--max-connections',
      String(options.maxConnections),
      '--max-body-bytes',
      String(options.maxBodyBytes),
      '--overview-cache-ms',
      String(options.overviewCacheMs),
    ],
    {
      env: cleanChildEnv({ MCPACE_TOOL_LIST_WARMUP: '0' }),
      stdio: ['ignore', 'pipe', 'pipe'],
    },
  );

  let stdout = '';
  let stderr = '';
  const ready = new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`server did not become ready. stderr: ${stderr}`)), 10_000);
    child.stdout.on('data', (chunk) => {
      stdout += chunk.toString('utf8');
      const match = stdout.match(/Server running at http:\/\/127\.0\.0\.1:(\d+)/);
      if (match) {
        clearTimeout(timer);
        resolve(Number(match[1]));
      }
    });
    child.stderr.on('data', (chunk) => {
      stderr += chunk.toString('utf8');
    });
    child.once('exit', (code, signal) => {
      clearTimeout(timer);
      reject(new Error(`server exited before ready: code=${code} signal=${signal} stderr=${stderr}`));
    });
  });

  const port = await ready;
  return {
    root,
    port,
    child,
    stop: async () => {
      if (child.exitCode === null && !child.killed) {
        child.kill('SIGTERM');
        await new Promise((resolve) => child.once('exit', resolve));
      }
      if (!options.root) await rm(root, { recursive: true, force: true });
    },
  };
}

function requestOnce({ port, method = 'GET', target, headers = {}, body = '' }) {
  const bodyBuffer = Buffer.from(body);
  const requestHeaders = { ...headers };
  if (bodyBuffer.length && !requestHeaders['Content-Length']) {
    requestHeaders['Content-Length'] = String(bodyBuffer.length);
  }
  return new Promise((resolve) => {
    const started = performance.now();
    const request = http.request(
      {
        host: '127.0.0.1',
        port,
        method,
        path: target,
        headers: requestHeaders,
        agent: false,
      },
      (response) => {
        response.resume();
        response.on('end', () => {
          resolve({
            ok: true,
            status: response.statusCode || 0,
            latencyMs: performance.now() - started,
            headers: response.headers,
          });
        });
      },
    );
    request.setTimeout(5_000, () => request.destroy(new Error('request timeout')));
    request.on('error', (error) => {
      resolve({ ok: false, error: error.message, status: 0, latencyMs: performance.now() - started });
    });
    if (bodyBuffer.length) request.write(bodyBuffer);
    request.end();
  });
}

async function runScenario({ port, name, method, target, headers, body, durationMs, concurrency, expectedStatuses }) {
  const deadline = performance.now() + durationMs;
  const latencies = [];
  const statusCounts = new Map();
  const errorCounts = new Map();
  let total = 0;
  let failed = 0;

  async function worker() {
    while (performance.now() < deadline) {
      const result = await requestOnce({ port, method, target, headers, body });
      total += 1;
      latencies.push(result.latencyMs);
      statusCounts.set(result.status, (statusCounts.get(result.status) || 0) + 1);
      if (!result.ok || !expectedStatuses.includes(result.status)) {
        failed += 1;
        const key = result.ok ? `unexpected-status-${result.status}` : result.error;
        errorCounts.set(key, (errorCounts.get(key) || 0) + 1);
      }
    }
  }

  const started = performance.now();
  await Promise.all(Array.from({ length: concurrency }, () => worker()));
  const elapsedSeconds = Math.max((performance.now() - started) / 1000, 0.001);
  latencies.sort((a, b) => a - b);
  return {
    name,
    method,
    target,
    durationMs: Math.round(elapsedSeconds * 1000),
    concurrency,
    requests: total,
    failed,
    rps: round(total / elapsedSeconds),
    latencyMs: latencySummary(latencies),
    statuses: Object.fromEntries([...statusCounts.entries()].sort((a, b) => a[0] - b[0])),
    errors: Object.fromEntries([...errorCounts.entries()].sort()),
  };
}

function latencySummary(values) {
  if (!values.length) return { min: 0, avg: 0, p50: 0, p95: 0, p99: 0, max: 0 };
  const sum = values.reduce((acc, value) => acc + value, 0);
  return {
    min: round(values[0]),
    avg: round(sum / values.length),
    p50: round(percentile(values, 0.5)),
    p95: round(percentile(values, 0.95)),
    p99: round(percentile(values, 0.99)),
    max: round(values[values.length - 1]),
  };
}

function percentile(values, fraction) {
  const index = Math.min(values.length - 1, Math.ceil(values.length * fraction) - 1);
  return values[Math.max(index, 0)];
}

function round(value) {
  return Math.round(value * 100) / 100;
}

function initializeBody(id = 1) {
  return JSON.stringify({
    jsonrpc: '2.0',
    id,
    method: 'initialize',
    params: {
      protocolVersion: '2025-06-18',
      capabilities: {},
      clientInfo: { name: 'mcpace-local-load-test', version: '0.0.0' },
    },
  });
}

async function runEdgeProbes(port, maxBodyBytes) {
  const mcpHeaders = {
    Accept: 'application/json, text/event-stream',
    'Content-Type': 'application/json',
  };
  const probes = [
    {
      name: 'rejects spoofed Host header',
      request: { method: 'GET', target: '/healthz', headers: { Host: '127.0.0.1.evil.example' } },
      expected: [403],
    },
    {
      name: 'rejects cross-origin MCP POST',
      request: {
        method: 'POST',
        target: '/mcp',
        headers: { ...mcpHeaders, Origin: 'http://localhost.evil.example' },
        body: initializeBody(2),
      },
      expected: [403],
    },
    {
      name: 'rejects MCP POST without streamable Accept',
      request: { method: 'POST', target: '/mcp', headers: { 'Content-Type': 'application/json' }, body: initializeBody(3) },
      expected: [400],
    },
    {
      name: 'rejects over-limit MCP body',
      request: { method: 'POST', target: '/mcp', headers: mcpHeaders, body: 'x'.repeat(maxBodyBytes + 1024) },
      expected: [413],
      expectedErrors: [/ECONNRESET/i, /socket hang up/i],
    },
    {
      name: 'rejects unknown MCP session id',
      request: {
        method: 'POST',
        target: '/mcp',
        headers: { ...mcpHeaders, 'Mcp-Session-Id': 'mcpace-unknown-session' },
        body: JSON.stringify({ jsonrpc: '2.0', id: 4, method: 'tools/list', params: {} }),
      },
      expected: [404],
    },
  ];

  const results = [];
  for (const probe of probes) {
    const result = await requestOnce({ port, ...probe.request });
    const expectedError = !result.ok
      && (probe.expectedErrors || []).some((pattern) => pattern.test(result.error || ''));
    results.push({
      name: probe.name,
      expected: probe.expected,
      status: result.status,
      pass: (result.ok && probe.expected.includes(result.status)) || expectedError,
      latencyMs: round(result.latencyMs),
      error: result.error || '',
    });
  }
  return results;
}

function printText(summary) {
  console.log(`MCPace local load test against ${summary.baseUrl}`);
  console.log(`binary: ${summary.binary}`);
  console.log(`root: ${summary.root}`);
  for (const scenario of summary.scenarios) {
    console.log(`\n${scenario.name}: ${scenario.requests} requests, ${scenario.rps} req/s, failed=${scenario.failed}`);
    console.log(`  latency ms: avg=${scenario.latencyMs.avg} p50=${scenario.latencyMs.p50} p95=${scenario.latencyMs.p95} p99=${scenario.latencyMs.p99} max=${scenario.latencyMs.max}`);
    console.log(`  statuses: ${JSON.stringify(scenario.statuses)}`);
  }
  console.log('\nEdge probes:');
  for (const probe of summary.edgeProbes) {
    console.log(`  ${probe.pass ? 'PASS' : 'FAIL'} ${probe.name}: status=${probe.status}, expected=${probe.expected.join('|')}`);
  }
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const server = await startServer(options);
  try {
    const common = { port: server.port, durationMs: options.durationMs, concurrency: options.concurrency };
    const scenarios = [];
    scenarios.push(
      await runScenario({
        ...common,
        name: 'healthz readiness endpoint',
        method: 'GET',
        target: '/healthz',
        expectedStatuses: [200],
      }),
    );
    scenarios.push(
      await runScenario({
        ...common,
        name: 'cached overview endpoint',
        method: 'GET',
        target: '/api/overview',
        expectedStatuses: [200],
      }),
    );
    scenarios.push(
      await runScenario({
        ...common,
        name: 'MCP initialize POST',
        method: 'POST',
        target: '/mcp',
        headers: {
          Accept: 'application/json, text/event-stream',
          'Content-Type': 'application/json',
        },
        body: initializeBody(),
        expectedStatuses: [200],
      }),
    );
    const edgeProbes = await runEdgeProbes(server.port, options.maxBodyBytes);
    const summary = {
      generatedAt: new Date().toISOString(),
      binary: options.binary,
      root: server.root,
      baseUrl: `http://127.0.0.1:${server.port}`,
      options: {
        durationMs: options.durationMs,
        concurrency: options.concurrency,
        port: options.port || 'auto',
        maxConnections: options.maxConnections,
        maxBodyBytes: options.maxBodyBytes,
        overviewCacheMs: options.overviewCacheMs,
      },
      scenarios,
      edgeProbes,
      passed: scenarios.every((scenario) => scenario.failed === 0) && edgeProbes.every((probe) => probe.pass),
    };
    if (options.json) console.log(JSON.stringify(summary, null, 2));
    else printText(summary);
    process.exitCode = summary.passed ? 0 : 1;
  } finally {
    await server.stop();
  }
}

main().catch((error) => {
  console.error(error?.stack || error?.message || String(error));
  process.exit(1);
});
