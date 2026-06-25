#!/usr/bin/env node
import {
  boundedPositiveInteger,
  defaultBinary,
  explicitBinaryFromEnv,
  httpRequest,
  initializeBody,
  jsonRequest,
  latencySummary,
  mcpPost,
  positiveInteger,
  round,
  startMcpaceServer,
} from './lib/runtime-probe.mjs';

function printHelp() {
  console.log(`Usage: node scripts/session-churn-probe.mjs [options]\n\nStarts a local MCPace server and exercises many MCP Streamable HTTP sessions.\n\nOptions:\n  --binary <path>           MCPace binary. Env fallback: MCPACE_BINARY, MCPACE_BINARY_PATH, MCPACE_DEV_BINARY\n  --root <path>             Existing MCPace root. Omit to create an isolated temporary root\n  --port <n>                Server port. Default: free loopback port\n  --sessions <n>            Sessions to create. Default: 200\n  --batch-size <n>          Concurrent session workers. Default: 32\n  --max-connections <n>     Server-side connection cap. Default: 128; max: 256\n  --io-timeout-ms <n>       Server read/write timeout. Default: 5000\n  --request-timeout-ms <n>  Client request timeout. Default: 5000\n  --json                   Emit JSON only`);
}

function parseArgs(argv) {
  const options = {
    binary: explicitBinaryFromEnv() || defaultBinary(),
    root: '',
    port: 0,
    sessions: 200,
    batchSize: 32,
    maxConnections: 128,
    ioTimeoutMs: 5_000,
    requestTimeoutMs: 5_000,
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
      case '--sessions': options.sessions = positiveInteger(readValue(), arg); break;
      case '--batch-size': options.batchSize = positiveInteger(readValue(), arg); break;
      case '--max-connections': options.maxConnections = boundedPositiveInteger(readValue(), arg, 256); break;
      case '--io-timeout-ms': options.ioTimeoutMs = positiveInteger(readValue(), arg); break;
      case '--request-timeout-ms': options.requestTimeoutMs = positiveInteger(readValue(), arg); break;
      case '--json': options.json = true; break;
      case '-h':
      case '--help': options.help = true; break;
      default: throw new Error(`unknown argument: ${arg}`);
    }
  }
  options.batchSize = Math.min(options.batchSize, options.sessions);
  return options;
}

function statusCounter() {
  const map = new Map();
  return {
    add(status) { map.set(status, (map.get(status) || 0) + 1); },
    object() { return Object.fromEntries([...map.entries()].sort((a, b) => a[0] - b[0])); },
  };
}

async function runSession(port, index, timeoutMs) {
  const initialize = await mcpPost({ port, body: initializeBody(index + 1, `mcpace-session-churn-${index}`), timeoutMs });
  const sessionId = initialize.headers?.['mcp-session-id'] || '';
  const protocol = initialize.headers?.['mcp-protocol-version'] || '2025-06-18';
  let initialized = { ok: false, status: 0, latencyMs: 0, error: 'initialize did not return session id' };
  let toolsList = { ok: false, status: 0, latencyMs: 0, error: 'initialize did not return session id' };
  if (initialize.ok && initialize.status === 200 && sessionId) {
    initialized = await mcpPost({
      port,
      sessionId,
      protocolVersion: protocol,
      body: JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' }),
      timeoutMs,
    });
    toolsList = await mcpPost({
      port,
      sessionId,
      protocolVersion: protocol,
      body: JSON.stringify({ jsonrpc: '2.0', id: 10_000 + index, method: 'tools/list', params: {} }),
      timeoutMs,
    });
  }
  return { initialize, initialized, toolsList, sessionId: Boolean(sessionId) };
}

async function runProbe(options) {
  const server = await startMcpaceServer({
    binary: options.binary,
    root: options.root,
    port: options.port,
    maxConnections: options.maxConnections,
    ioTimeoutMs: options.ioTimeoutMs,
    rootPrefix: 'mcpace-session-churn-',
  });
  try {
    const started = Date.now();
    const initializeStatuses = statusCounter();
    const initializedStatuses = statusCounter();
    const toolsListStatuses = statusCounter();
    const initializeLatencies = [];
    const initializedLatencies = [];
    const toolsListLatencies = [];
    let sessionsWithHeader = 0;
    let failed = 0;
    let nextIndex = 0;

    async function worker() {
      while (nextIndex < options.sessions) {
        const index = nextIndex;
        nextIndex += 1;
        const result = await runSession(server.port, index, options.requestTimeoutMs);
        initializeStatuses.add(result.initialize.status);
        initializedStatuses.add(result.initialized.status);
        toolsListStatuses.add(result.toolsList.status);
        initializeLatencies.push(result.initialize.latencyMs);
        initializedLatencies.push(result.initialized.latencyMs);
        toolsListLatencies.push(result.toolsList.latencyMs);
        if (result.sessionId) sessionsWithHeader += 1;
        if (!result.initialize.ok || result.initialize.status !== 200 || !result.sessionId) failed += 1;
        if (!result.initialized.ok || result.initialized.status !== 202) failed += 1;
        if (!result.toolsList.ok || result.toolsList.status !== 200) failed += 1;
      }
    }

    await Promise.all(Array.from({ length: options.batchSize }, () => worker()));
    const serverRuntime = await jsonRequest({ port: server.port, target: '/api/resources', timeoutMs: options.requestTimeoutMs });
    const elapsedMs = Date.now() - started;
    const summary = {
      schema: 'mcpace.sessionChurnProbe.v1',
      generatedAt: new Date().toISOString(),
      binary: server.binary,
      root: server.root,
      baseUrl: `http://127.0.0.1:${server.port}`,
      options: {
        sessions: options.sessions,
        batchSize: options.batchSize,
        maxConnections: options.maxConnections,
        ioTimeoutMs: options.ioTimeoutMs,
        requestTimeoutMs: options.requestTimeoutMs,
      },
      elapsedMs,
      sessionsPerSecond: round(options.sessions / Math.max(elapsedMs / 1000, 0.001)),
      sessionsWithHeader,
      failed,
      passed: failed === 0 && sessionsWithHeader === options.sessions && serverRuntime.ok,
      statuses: {
        initialize: initializeStatuses.object(),
        initializedNotification: initializedStatuses.object(),
        toolsList: toolsListStatuses.object(),
      },
      latencyMs: {
        initialize: latencySummary(initializeLatencies),
        initializedNotification: latencySummary(initializedLatencies),
        toolsList: latencySummary(toolsListLatencies),
      },
      serverRuntime,
    };
    return summary;
  } finally {
    await server.stop();
  }
}

function printText(summary) {
  console.log(`MCPace session churn probe: ${summary.passed ? 'PASS' : 'FAIL'}`);
  console.log(`sessions=${summary.options.sessions} batch=${summary.options.batchSize} failed=${summary.failed} withSessionHeader=${summary.sessionsWithHeader}`);
  console.log(`initialize: p95=${summary.latencyMs.initialize.p95}ms p99=${summary.latencyMs.initialize.p99}ms statuses=${JSON.stringify(summary.statuses.initialize)}`);
  console.log(`initialized notification: p95=${summary.latencyMs.initializedNotification.p95}ms p99=${summary.latencyMs.initializedNotification.p99}ms statuses=${JSON.stringify(summary.statuses.initializedNotification)}`);
  console.log(`tools/list: p95=${summary.latencyMs.toolsList.p95}ms p99=${summary.latencyMs.toolsList.p99}ms statuses=${JSON.stringify(summary.statuses.toolsList)}`);
  const sessions = summary.serverRuntime?.payload?.runtime?.httpSessionStore;
  if (sessions) console.log(`server sessions: count=${sessions.size}/${sessions.maxSize} pruned=${sessions.prunedExpiredSessions}`);
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
