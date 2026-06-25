#!/usr/bin/env node
import net from 'node:net';
import { performance } from 'node:perf_hooks';

function printHelp() {
  console.log(`Usage: node scripts/slow-client-probe.mjs --base-url http://127.0.0.1:<port> [--json] [--timeout-ms <n>] [--byte-delay-ms <n>]\n\nRuns raw-socket HTTP fault probes against an already running MCPace dashboard/serve endpoint. It checks slow headers, incomplete bodies, overlong headers, and duplicate Content-Length handling.`);
}

function parseArgs(argv) {
  const getValue = (name, fallback = '') => {
    const index = argv.indexOf(name);
    return index >= 0 ? argv[index + 1] || fallback : fallback;
  };
  return {
    help: argv.includes('-h') || argv.includes('--help'),
    json: argv.includes('--json'),
    baseUrl: getValue('--base-url'),
    timeoutMs: positiveInteger(getValue('--timeout-ms', '7000'), '--timeout-ms'),
    byteDelayMs: positiveInteger(getValue('--byte-delay-ms', '10'), '--byte-delay-ms'),
  };
}

function positiveInteger(value, label) {
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) throw new Error(`${label} must be a positive integer`);
  return parsed;
}

function parseBaseUrl(value) {
  if (!value) throw new Error('--base-url is required');
  const url = new URL(value);
  if (!['http:', 'https:'].includes(url.protocol)) throw new Error('--base-url must be http:// or https://');
  if (url.protocol === 'https:') throw new Error('raw slow-client probes currently support http:// only');
  return { host: url.hostname, port: Number(url.port || '80') };
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function rawRequest({ host, port, chunks, delayMs = 0, timeoutMs, end = true }) {
  return new Promise((resolve) => {
    const started = performance.now();
    let data = '';
    let settled = false;
    const socket = net.createConnection({ host, port });
    const finish = (result) => {
      if (settled) return;
      settled = true;
      socket.destroy();
      resolve({ ...result, latencyMs: round(performance.now() - started), rawHead: data.slice(0, 512) });
    };
    const timer = setTimeout(() => finish({ ok: false, status: 0, outcome: 'timeout' }), timeoutMs);
    socket.setEncoding('utf8');
    socket.on('connect', async () => {
      try {
        for (const chunk of chunks) {
          socket.write(chunk);
          if (delayMs > 0) await sleep(delayMs);
        }
        if (end) socket.end();
      } catch (error) {
        clearTimeout(timer);
        finish({ ok: false, status: 0, outcome: error?.message || String(error) });
      }
    });
    socket.on('data', (chunk) => {
      data += chunk;
      const match = data.match(/^HTTP\/\d\.\d\s+(\d+)/);
      if (match) {
        clearTimeout(timer);
        finish({ ok: true, status: Number(match[1]), outcome: 'response' });
      }
    });
    socket.on('error', (error) => {
      clearTimeout(timer);
      finish({ ok: false, status: 0, outcome: error.message });
    });
    socket.on('close', () => {
      clearTimeout(timer);
      if (!settled) {
        const match = data.match(/^HTTP\/\d\.\d\s+(\d+)/);
        finish({ ok: Boolean(match), status: match ? Number(match[1]) : 0, outcome: match ? 'response' : 'closed' });
      }
    });
  });
}

function healthRequest(host) {
  return `GET /healthz HTTP/1.1\r\nHost: ${host}\r\nConnection: close\r\n\r\n`;
}

function mcpHeaders(host, extra = '') {
  return `POST /mcp HTTP/1.1\r\nHost: ${host}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\n${extra}`;
}

function initializeBody() {
  return JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'mcpace-slow-probe', version: '0.0.0' } } });
}

async function runProbes(options) {
  const endpoint = parseBaseUrl(options.baseUrl);
  const hostHeader = endpoint.host.includes(':') ? `[${endpoint.host}]` : endpoint.host;
  const health = healthRequest(hostHeader);
  const slowHeaderChunks = health.split('');
  const body = initializeBody();
  const probes = [
    {
      name: 'normal health request over raw socket',
      expected: [200],
      run: () => rawRequest({ ...endpoint, chunks: [health], timeoutMs: options.timeoutMs }),
    },
    {
      name: 'slow byte-by-byte health headers remain bounded by server timeout',
      expected: [200, 400, 408],
      allowOutcomes: ['response', 'closed', 'timeout'],
      run: () => rawRequest({ ...endpoint, chunks: slowHeaderChunks, delayMs: options.byteDelayMs, timeoutMs: options.timeoutMs }),
    },
    {
      name: 'duplicate content-length is rejected',
      expected: [400],
      run: () => rawRequest({ ...endpoint, chunks: [`${mcpHeaders(hostHeader, 'Content-Length: 2\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}')}`], timeoutMs: options.timeoutMs }),
    },
    {
      name: 'overlong header is rejected',
      expected: [431],
      run: () => rawRequest({ ...endpoint, chunks: [`GET /healthz HTTP/1.1\r\nHost: ${hostHeader}\r\nX-MCPace-Probe: ${'x'.repeat(20_000)}\r\nConnection: close\r\n\r\n`], timeoutMs: options.timeoutMs }),
    },
    {
      name: 'declared MCP body without complete payload does not produce success',
      expected: [400, 408, 500],
      allowOutcomes: ['response', 'closed', 'timeout'],
      run: () => rawRequest({ ...endpoint, chunks: [`${mcpHeaders(hostHeader, 'Content-Length: 999999\r\nConnection: close\r\n\r\n')}{`], timeoutMs: options.timeoutMs, end: false }),
    },
    {
      name: 'valid MCP initialize still succeeds after fault probes',
      expected: [200],
      run: () => rawRequest({ ...endpoint, chunks: [`${mcpHeaders(hostHeader, `Content-Length: ${Buffer.byteLength(body)}\r\nConnection: close\r\n\r\n`)}${body}`], timeoutMs: options.timeoutMs }),
    },
  ];

  const results = [];
  for (const probe of probes) {
    const result = await probe.run();
    const pass = (result.ok && probe.expected.includes(result.status))
      || (!result.ok && (probe.allowOutcomes || []).includes(result.outcome));
    results.push({ name: probe.name, expected: probe.expected, pass, ...result });
  }
  return {
    schema: 'mcpace.slowClientProbe.v1',
    generatedAt: new Date().toISOString(),
    baseUrl: options.baseUrl,
    options: { timeoutMs: options.timeoutMs, byteDelayMs: options.byteDelayMs },
    passed: results.every((probe) => probe.pass),
    probes: results,
  };
}

function round(value) {
  return Math.round(value * 100) / 100;
}

function printText(report) {
  console.log(`MCPace slow-client probes: ${report.passed ? 'pass' : 'failed'} (${report.baseUrl})`);
  for (const probe of report.probes) {
    console.log(`- ${probe.pass ? 'PASS' : 'FAIL'} ${probe.name}: status=${probe.status} outcome=${probe.outcome} latency=${probe.latencyMs}ms`);
  }
}

try {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    printHelp();
  } else {
    const report = await runProbes(args);
    if (args.json) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
    else printText(report);
    if (!report.passed) process.exitCode = 1;
  }
} catch (error) {
  process.stderr.write(`${error?.message || String(error)}\n`);
  process.exitCode = 2;
}
