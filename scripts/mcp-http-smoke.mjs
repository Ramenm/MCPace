#!/usr/bin/env node
import http from 'node:http';
import https from 'node:https';
import fs from 'node:fs';
import path from 'node:path';

function parseArgs(argv) {
  const args = {
    url: 'http://127.0.0.1:39022/mcp',
    healthUrl: 'http://127.0.0.1:39022/healthz',
    expectTool: 'hub_status',
    checkGet: true,
    json: false,
    write: null,
  };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--url') args.url = argv[++i];
    else if (arg === '--health-url') args.healthUrl = argv[++i];
    else if (arg === '--expect-tool') args.expectTool = argv[++i];
    else if (arg === '--no-get-check') args.checkGet = false;
    else if (arg === '--json') args.json = true;
    else if (arg === '--write') args.write = path.resolve(argv[++i] ?? '');
    else if (arg === '--help' || arg === '-h') {
      console.log(`Usage: node scripts/mcp-http-smoke.mjs [--url http://127.0.0.1:39022/mcp] [--json]\n\nChecks MCP Streamable HTTP initialize, MCP-Session-Id forwarding, initialized notification, tools/list, and optional GET rejection.`);
      process.exit(0);
    } else {
      throw new Error(`Unknown argument: ${arg}`);
    }
  }
  return args;
}

function request(urlString, { method = 'GET', headers = {}, body = null, timeoutMs = 10_000 } = {}) {
  const url = new URL(urlString);
  const transport = url.protocol === 'https:' ? https : http;
  return new Promise((resolve, reject) => {
    const req = transport.request(url, { method, headers, timeout: timeoutMs }, (res) => {
      const chunks = [];
      res.on('data', (chunk) => chunks.push(chunk));
      res.on('end', () => {
        resolve({
          statusCode: res.statusCode,
          statusMessage: res.statusMessage,
          headers: res.headers,
          body: Buffer.concat(chunks).toString('utf8'),
        });
      });
    });
    req.on('timeout', () => {
      req.destroy(new Error(`request timed out after ${timeoutMs}ms`));
    });
    req.on('error', reject);
    if (body !== null) req.write(body);
    req.end();
  });
}

function parseJsonMaybe(body) {
  const trimmed = String(body || '').trim();
  if (!trimmed) return null;
  const candidate = trimmed.startsWith('event:')
    ? trimmed.split('\n').find((line) => line.startsWith('data:'))?.slice(5).trim()
    : trimmed;
  if (!candidate) return null;
  return JSON.parse(candidate);
}

async function postMcp(url, payload, sessionId = null) {
  const body = JSON.stringify(payload);
  const headers = {
    Accept: 'application/json, text/event-stream',
    'Content-Type': 'application/json',
    'Content-Length': Buffer.byteLength(body),
  };
  if (sessionId) headers['MCP-Session-Id'] = sessionId;
  return request(url, { method: 'POST', headers, body });
}

function add(checks, id, status, summary, detail = '', meta = {}) {
  checks.push({ id, status, summary, detail, meta });
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const checks = [];
  try {
    const health = await request(args.healthUrl, { method: 'GET', timeoutMs: 3000 });
    add(checks, 'healthz', health.statusCode && health.statusCode < 500 ? 'pass' : 'warn', `healthz returned ${health.statusCode}`, health.body.slice(0, 1000), { statusCode: health.statusCode });
  } catch (err) {
    add(checks, 'healthz', 'warn', 'healthz request failed', String(err.message || err));
  }

  const initialize = {
    jsonrpc: '2.0',
    id: 1,
    method: 'initialize',
    params: {
      protocolVersion: '2025-11-25',
      capabilities: {},
      clientInfo: { name: 'mcpace-http-smoke', version: '1.0.0' },
    },
  };
  const initResp = await postMcp(args.url, initialize);
  const initBody = parseJsonMaybe(initResp.body);
  const sessionId = initResp.headers['mcp-session-id'] || initResp.headers['MCP-Session-Id'];
  const initOk = initResp.statusCode === 200 && initBody?.result?.protocolVersion;
  add(checks, 'mcp.initialize', initOk ? 'pass' : 'fail', initOk ? 'initialize returned protocolVersion' : `initialize failed with HTTP ${initResp.statusCode}`, initResp.body.slice(0, 4000), { statusCode: initResp.statusCode, sessionId: sessionId || null });

  const initializedResp = await postMcp(args.url, { jsonrpc: '2.0', method: 'notifications/initialized' }, sessionId || null);
  const initializedOk = initializedResp.statusCode === 202 || initializedResp.statusCode === 200;
  add(checks, 'mcp.initialized', initializedOk ? 'pass' : 'fail', initializedOk ? 'initialized notification accepted' : `initialized notification failed with HTTP ${initializedResp.statusCode}`, initializedResp.body.slice(0, 4000), { statusCode: initializedResp.statusCode, sessionIdSent: Boolean(sessionId) });

  if (args.checkGet) {
    const getResp = await request(args.url, { method: 'GET', headers: { Accept: 'text/event-stream' }, timeoutMs: 5000 });
    const getOk = [400, 404, 405].includes(getResp.statusCode);
    add(checks, 'mcp.get-boundary', getOk ? 'pass' : 'warn', getOk ? `GET boundary returned ${getResp.statusCode}` : `GET returned unexpected ${getResp.statusCode}`, getResp.body.slice(0, 2000), { statusCode: getResp.statusCode });
  }

  const toolsResp = await postMcp(args.url, { jsonrpc: '2.0', id: 2, method: 'tools/list', params: {} }, sessionId || null);
  let toolsBody = null;
  try { toolsBody = parseJsonMaybe(toolsResp.body); } catch {}
  const toolNames = Array.isArray(toolsBody?.result?.tools) ? toolsBody.result.tools.map((tool) => tool.name) : [];
  const toolsOk = toolsResp.statusCode === 200 && (!args.expectTool || toolNames.includes(args.expectTool));
  add(checks, 'mcp.tools-list', toolsOk ? 'pass' : 'fail', toolsOk ? `tools/list includes ${args.expectTool}` : `tools/list failed or missing ${args.expectTool}`, toolsResp.body.slice(0, 6000), { statusCode: toolsResp.statusCode, toolNames });

  const counts = checks.reduce((acc, check) => {
    acc[check.status] = (acc[check.status] || 0) + 1;
    return acc;
  }, {});
  const status = counts.fail ? 'fail' : counts.warn ? 'warn' : 'pass';
  const report = { generatedAt: new Date().toISOString(), url: args.url, healthUrl: args.healthUrl, summary: { status, counts }, checks };
  if (args.write) {
    fs.mkdirSync(path.dirname(args.write), { recursive: true });
    fs.writeFileSync(args.write, `${JSON.stringify(report, null, 2)}\n`);
  }
  if (args.json) console.log(JSON.stringify(report, null, 2));
  else {
    console.log(`MCP HTTP smoke: ${status.toUpperCase()}`);
    for (const check of checks) console.log(`${check.status.toUpperCase()} ${check.id}: ${check.summary}`);
  }
  process.exitCode = status === 'fail' ? 1 : 0;
}

main().catch((err) => {
  console.error(err?.stack || err?.message || String(err));
  process.exit(2);
});
