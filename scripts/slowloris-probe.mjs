#!/usr/bin/env node
import net from 'node:net';
import { performance } from 'node:perf_hooks';
import {
  boundedPositiveInteger,
  defaultBinary,
  explicitBinaryFromEnv,
  jsonRequest,
  latencySummary,
  positiveInteger,
  round,
  startMcpaceServer,
} from './lib/runtime-probe.mjs';

function printHelp() {
  console.log(`Usage: node scripts/slowloris-probe.mjs [options]\n\nStarts a local MCPace server and checks that slow or half-open TCP clients are closed by IO timeouts.\n\nOptions:\n  --binary <path>           MCPace binary. Env fallback: MCPACE_BINARY, MCPACE_BINARY_PATH, MCPACE_DEV_BINARY\n  --root <path>             Existing MCPace root. Omit to create an isolated temporary root\n  --port <n>                Server port. Default: free loopback port\n  --connections <n>         Connections per slow-client mode. Default: 16\n  --max-connections <n>     Server-side connection cap. Default: 64; max: 256\n  --io-timeout-ms <n>       Server read/write timeout. Default: 1000\n  --client-deadline-ms <n>  Probe deadline per socket. Default: io-timeout-ms*4+2000\n  --json                   Emit JSON only`);
}

function parseArgs(argv) {
  const options = {
    binary: explicitBinaryFromEnv() || defaultBinary(),
    root: '',
    port: 0,
    connections: 16,
    maxConnections: 64,
    ioTimeoutMs: 1_000,
    clientDeadlineMs: 0,
    json: false,
    help: false,
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
      case '--binary': options.binary = readValue(); break;
      case '--root': options.root = readValue(); break;
      case '--port': options.port = positiveInteger(readValue(), arg); break;
      case '--connections': options.connections = positiveInteger(readValue(), arg); break;
      case '--max-connections': options.maxConnections = boundedPositiveInteger(readValue(), arg, 256); break;
      case '--io-timeout-ms': options.ioTimeoutMs = positiveInteger(readValue(), arg); break;
      case '--client-deadline-ms': options.clientDeadlineMs = positiveInteger(readValue(), arg); break;
      case '--json': options.json = true; break;
      case '-h':
      case '--help': options.help = true; break;
      default: throw new Error(`unknown argument: ${arg}`);
    }
  }
  if (!options.clientDeadlineMs) options.clientDeadlineMs = options.ioTimeoutMs * 4 + 2_000;
  return options;
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function connectSocket(port, connectTimeoutMs = 2_000) {
  return new Promise((resolve) => {
    const started = performance.now();
    const socket = net.createConnection({ host: '127.0.0.1', port });
    const timer = setTimeout(() => {
      socket.destroy();
      resolve({ ok: false, elapsedMs: round(performance.now() - started), error: 'connect timeout' });
    }, connectTimeoutMs);
    socket.once('connect', () => {
      clearTimeout(timer);
      resolve({ ok: true, socket, started });
    });
    socket.once('error', (error) => {
      clearTimeout(timer);
      resolve({ ok: false, elapsedMs: round(performance.now() - started), error: error?.message || String(error) });
    });
  });
}

function waitForClose(socket, started, deadlineMs, extra = {}) {
  return new Promise((resolve) => {
    let settled = false;
    const finish = (closedByServer, error = '') => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      socket.destroy();
      resolve({
        connected: true,
        closedByServer,
        elapsedMs: round(performance.now() - started),
        error,
        ...extra,
      });
    };
    const timer = setTimeout(() => finish(false, 'client deadline reached'), deadlineMs);
    socket.on('data', () => {});
    socket.once('error', (error) => finish(true, error?.message || String(error)));
    socket.once('close', () => finish(true));
  });
}

async function runIdleConnection(port, deadlineMs) {
  const opened = await connectSocket(port);
  if (!opened.ok) return { connected: false, closedByServer: true, elapsedMs: opened.elapsedMs, error: opened.error };
  return waitForClose(opened.socket, opened.started, deadlineMs);
}

async function runIncompleteBody(port, deadlineMs) {
  const opened = await connectSocket(port);
  if (!opened.ok) return { connected: false, closedByServer: true, elapsedMs: opened.elapsedMs, error: opened.error };
  opened.socket.write('POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nContent-Length: 65535\r\n\r\n{');
  return waitForClose(opened.socket, opened.started, deadlineMs);
}

async function runDripHeaders(port, deadlineMs, intervalMs) {
  const opened = await connectSocket(port);
  if (!opened.ok) return { connected: false, closedByServer: true, elapsedMs: opened.elapsedMs, error: opened.error };
  const payload = 'GET /healthz HTTP/1.1\r\nHost: 127.0.0.1\r\nUser-Agent: mcpace-slowloris-probe\r\n';
  let index = 0;
  const writer = setInterval(() => {
    if (index >= payload.length) {
      clearInterval(writer);
      return;
    }
    opened.socket.write(payload[index]);
    index += 1;
  }, intervalMs);
  const result = await waitForClose(opened.socket, opened.started, deadlineMs, { bytesWritten: () => index });
  clearInterval(writer);
  result.bytesWritten = index;
  return result;
}

async function runMode(name, count, task) {
  const results = await Promise.all(Array.from({ length: count }, () => task()));
  const failed = results.filter((result) => !result.closedByServer).length;
  return {
    name,
    count,
    failed,
    passed: failed === 0,
    closedByServer: count - failed,
    latencyMs: latencySummary(results.map((result) => result.elapsedMs || 0)),
    samples: results.slice(0, 5),
  };
}

async function runProbe(options) {
  const server = await startMcpaceServer({
    binary: options.binary,
    root: options.root,
    port: options.port,
    maxConnections: options.maxConnections,
    ioTimeoutMs: options.ioTimeoutMs,
    rootPrefix: 'mcpace-slowloris-',
  });
  try {
    const before = await jsonRequest({ port: server.port, target: '/healthz', timeoutMs: 5_000 });
    const modes = [];
    modes.push(await runMode('idle-open-socket', options.connections, () => runIdleConnection(server.port, options.clientDeadlineMs)));
    await sleep(50);
    modes.push(await runMode('incomplete-content-length-body', options.connections, () => runIncompleteBody(server.port, options.clientDeadlineMs)));
    await sleep(50);
    modes.push(await runMode('dripped-header-bytes', options.connections, () => runDripHeaders(server.port, options.clientDeadlineMs, Math.max(25, Math.floor(options.ioTimeoutMs / 4)))));
    const after = await jsonRequest({ port: server.port, target: '/healthz', timeoutMs: 5_000 });
    const serverRuntime = await jsonRequest({ port: server.port, target: '/api/resources', timeoutMs: 5_000 });
    const passed = Boolean(before.ok && after.ok && serverRuntime.ok && modes.every((mode) => mode.passed));
    return {
      schema: 'mcpace.slowlorisProbe.v1',
      generatedAt: new Date().toISOString(),
      binary: server.binary,
      root: server.root,
      baseUrl: `http://127.0.0.1:${server.port}`,
      options: {
        connections: options.connections,
        maxConnections: options.maxConnections,
        ioTimeoutMs: options.ioTimeoutMs,
        clientDeadlineMs: options.clientDeadlineMs,
      },
      passed,
      healthz: { before: { ok: before.ok, status: before.status }, after: { ok: after.ok, status: after.status } },
      modes,
      serverRuntime,
    };
  } finally {
    await server.stop();
  }
}

function printText(summary) {
  console.log(`MCPace slow-client probe: ${summary.passed ? 'PASS' : 'FAIL'}`);
  console.log(`healthz before=${summary.healthz.before.status} after=${summary.healthz.after.status}`);
  for (const mode of summary.modes) {
    console.log(`${mode.passed ? 'PASS' : 'FAIL'} ${mode.name}: closed=${mode.closedByServer}/${mode.count} p95CloseMs=${mode.latencyMs.p95}`);
  }
  const http = summary.serverRuntime?.payload?.runtime?.http;
  if (http) {
    console.log(`server metrics: active=${http.activeConnections} failed=${http.failedConnections} maxActive=${http.maxObservedActiveConnections}`);
  }
}

try {
  const options = parseArgs(process.argv.slice(2));
  if (options.help) {
    printHelp();
  } else {
    const summary = await runProbe(options);
    if (options.json) process.stdout.write(`${JSON.stringify(summary, null, 2)}\n`);
    else printText(summary);
    if (!summary.passed) process.exitCode = 1;
  }
} catch (error) {
  process.stderr.write(`${error?.message || String(error)}\n`);
  process.exitCode = 2;
}
