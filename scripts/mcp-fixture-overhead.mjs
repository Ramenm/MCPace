#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { performance } from 'node:perf_hooks';
import { cleanChildEnv } from './lib/safe-child-env.mjs';
import { repoRoot, deriveProjectName, deriveProjectVersion } from './lib/project-metadata.mjs';

const DEFAULT_WRITE = 'reports/mcp-fixture-overhead-latest.json';
const DEFAULT_MARKDOWN = 'reports/mcp-fixture-overhead-latest.md';
const FIXTURE = 'tests/fixtures/tiny-mcp-stdio-server.mjs';

function parseArgs(argv) {
  const args = { json: false, write: path.join(repoRoot, DEFAULT_WRITE), markdown: path.join(repoRoot, DEFAULT_MARKDOWN), coldRuns: 7, warmLists: 40, timeoutMs: 5000, help: false };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    const value = () => {
      const next = argv[index + 1];
      if (!next || next.startsWith('--')) throw new Error(`${token} requires a value`);
      index += 1;
      return next;
    };
    switch (token) {
      case '--json': args.json = true; break;
      case '--write': args.write = path.resolve(value()); break;
      case '--markdown': args.markdown = path.resolve(value()); break;
      case '--no-write': args.write = null; args.markdown = null; break;
      case '--cold-runs': args.coldRuns = intArg(value(), token, 1, 100); break;
      case '--warm-lists': args.warmLists = intArg(value(), token, 1, 2000); break;
      case '--timeout-ms': args.timeoutMs = intArg(value(), token, 100, 60000); break;
      case '--help':
      case '-h': args.help = true; break;
      default: throw new Error(`unsupported mcp-fixture-overhead argument: ${token}`);
    }
  }
  return args;
}

function intArg(raw, label, min, max) {
  const parsed = Number.parseInt(raw, 10);
  if (!Number.isSafeInteger(parsed) || parsed < min || parsed > max) throw new Error(`${label} must be an integer in [${min}, ${max}]`);
  return parsed;
}

function help() {
  console.log(`Usage: node scripts/mcp-fixture-overhead.mjs [--json] [--cold-runs N] [--warm-lists N]

Measures actual local MCP stdio overhead against the repository fixture server:
  - cold process spawn + initialize + notifications/initialized + tools/list
  - warm tools/list calls over one initialized stdio session

It does not install, start, or call arbitrary third-party MCP servers.`);
}

function q(values) {
  const sorted = values.filter(Number.isFinite).sort((a, b) => a - b);
  const pick = (p) => sorted[Math.max(0, Math.min(sorted.length - 1, Math.ceil((p / 100) * sorted.length) - 1))];
  const round = (n) => Number(n.toFixed(3));
  if (!sorted.length) return { count: 0, min: null, p50: null, p95: null, p99: null, max: null, avg: null };
  return { count: sorted.length, min: round(sorted[0]), p50: round(pick(50)), p95: round(pick(95)), p99: round(pick(99)), max: round(sorted.at(-1)), avg: round(sorted.reduce((a, b) => a + b, 0) / sorted.length) };
}

function redact(value) {
  return String(value || '').replace(/(token|api[_-]?key|secret|password|bearer)\s*[=:]\s*[^\s,'\"]+/gi, '$1=[REDACTED]').slice(0, 1200);
}

class Client {
  constructor(timeoutMs) {
    this.timeoutMs = timeoutMs;
    this.id = 1;
    this.pending = new Map();
    this.buffer = '';
    this.stderr = '';
    this.closed = false;
    this.child = spawn(process.execPath, [path.join(repoRoot, FIXTURE)], {
      cwd: repoRoot,
      env: cleanChildEnv({ CI: '1', NO_COLOR: '1' }),
      stdio: ['pipe', 'pipe', 'pipe'],
      windowsHide: true,
    });
    this.child.stdout.setEncoding('utf8');
    this.child.stderr.setEncoding('utf8');
    this.child.stdout.on('data', (chunk) => this.onStdout(chunk));
    this.child.stderr.on('data', (chunk) => { this.stderr += chunk; });
    this.child.on('error', (error) => this.rejectAll(error));
    this.child.on('close', (code, signal) => {
      this.closed = true;
      this.rejectAll(new Error(`fixture closed before response: code=${code} signal=${signal}`));
    });
  }

  onStdout(chunk) {
    this.buffer += chunk;
    for (;;) {
      const newline = this.buffer.indexOf('\n');
      if (newline < 0) return;
      const line = this.buffer.slice(0, newline).trim();
      this.buffer = this.buffer.slice(newline + 1);
      if (!line) continue;
      let msg;
      try { msg = JSON.parse(line); } catch (error) { this.rejectAll(new Error(`invalid fixture JSON: ${error.message}`)); continue; }
      const entry = this.pending.get(msg.id);
      if (!entry) continue;
      this.pending.delete(msg.id);
      if (msg.error) entry.reject(new Error(msg.error.message || JSON.stringify(msg.error)));
      else entry.resolve(msg.result);
    }
  }

  rejectAll(error) {
    for (const [id, entry] of this.pending.entries()) {
      this.pending.delete(id);
      entry.reject(error);
    }
  }

  request(method, params = {}) {
    const id = this.id++;
    const payload = { jsonrpc: '2.0', id, method, params };
    const promise = new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`timeout waiting for ${method}`));
      }, this.timeoutMs);
      this.pending.set(id, { resolve: (value) => { clearTimeout(timer); resolve(value); }, reject: (error) => { clearTimeout(timer); reject(error); } });
    });
    this.child.stdin.write(`${JSON.stringify(payload)}\n`);
    return promise;
  }

  notify(method, params = {}) {
    this.child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', method, params })}\n`);
  }

  async close() {
    if (this.closed) return;
    this.closed = true;
    this.child.stdin.end();
    this.child.kill('SIGTERM');
    await new Promise((resolve) => {
      const timer = setTimeout(resolve, 100);
      this.child.once('close', () => { clearTimeout(timer); resolve(); });
    });
  }
}

async function initialize(client) {
  const result = await client.request('initialize', { protocolVersion: '2025-03-26', capabilities: {}, clientInfo: { name: 'mcpace-fixture-overhead', version: deriveProjectVersion() } });
  client.notify('notifications/initialized');
  return result;
}

async function coldRun(index, timeoutMs) {
  const started = performance.now();
  const client = new Client(timeoutMs);
  try {
    const initStarted = performance.now();
    const init = await initialize(client);
    const initDone = performance.now();
    const listStarted = performance.now();
    const list = await client.request('tools/list', {});
    const done = performance.now();
    return { index, ok: true, protocolVersion: init.protocolVersion || null, toolCount: Array.isArray(list.tools) ? list.tools.length : 0, spawnInitializeMs: initDone - started, initializeRoundTripMs: initDone - initStarted, toolsListMs: done - listStarted, totalMs: done - started };
  } catch (error) {
    return { index, ok: false, error: redact(error.message), stderr: redact(client.stderr), totalMs: performance.now() - started };
  } finally {
    await client.close();
  }
}

async function measureCold(args) {
  const samples = [];
  for (let index = 0; index < args.coldRuns; index += 1) samples.push(await coldRun(index, args.timeoutMs));
  const ok = samples.filter((row) => row.ok);
  return { runs: args.coldRuns, failures: samples.length - ok.length, samples, stats: { spawnInitializeMs: q(ok.map((row) => row.spawnInitializeMs)), toolsListMs: q(ok.map((row) => row.toolsListMs)), totalMs: q(ok.map((row) => row.totalMs)) } };
}

async function measureWarm(args) {
  const client = new Client(args.timeoutMs);
  const samples = [];
  try {
    const initStarted = performance.now();
    await initialize(client);
    const initializedMs = performance.now() - initStarted;
    for (let index = 0; index < args.warmLists; index += 1) {
      const started = performance.now();
      try {
        const list = await client.request('tools/list', {});
        samples.push({ index, ok: true, toolCount: Array.isArray(list.tools) ? list.tools.length : 0, ms: performance.now() - started });
      } catch (error) {
        samples.push({ index, ok: false, error: redact(error.message), ms: performance.now() - started });
      }
    }
    const ok = samples.filter((row) => row.ok);
    return { iterations: args.warmLists, initializedMs: Number(initializedMs.toFixed(3)), failures: samples.length - ok.length, samples, stats: { toolsListMs: q(ok.map((row) => row.ms)) } };
  } finally {
    await client.close();
  }
}

function check(id, ok, severity, evidence, recommendation = '') {
  return { id, ok: Boolean(ok), status: ok ? 'pass' : 'fail', severity, evidence, recommendation };
}

function markdown(report) {
  const lines = ['# MCP fixture overhead', '', `- Status: ${report.status}`, `- Generated: ${report.generatedAt}`, `- Safety: starts third-party MCP servers = ${report.safety.startsThirdPartyMcpServers}, calls third-party tools = ${report.safety.callsThirdPartyTools}`, '', '## Measurements', '', '| Area | p50 | p95 | max |', '|---|---:|---:|---:|', `| Cold stdio total ms | ${report.cold.stats.totalMs.p50} | ${report.cold.stats.totalMs.p95} | ${report.cold.stats.totalMs.max} |`, `| Cold initialize ms | ${report.cold.stats.spawnInitializeMs.p50} | ${report.cold.stats.spawnInitializeMs.p95} | ${report.cold.stats.spawnInitializeMs.max} |`, `| Warm tools/list ms | ${report.warm.stats.toolsListMs.p50} | ${report.warm.stats.toolsListMs.p95} | ${report.warm.stats.toolsListMs.max} |`, '', '## Checks', '', '| Check | Status | Severity | Evidence |', '|---|---:|---|---|'];
  for (const row of report.checks) lines.push(`| ${row.id} | ${row.status} | ${row.severity} | ${String(row.evidence).replace(/[|\n\r]/g, ' ')} |`);
  lines.push('', '## Notes', '', '- This is actual MCP stdio lifecycle measurement against a local deterministic fixture, not a random package benchmark.', '- Cold start is expected to be much higher than warm `tools/list`; production should avoid paying cold stdio startup per user request whenever policy allows reuse/cache.');
  return `${lines.join('\n')}\n`;
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) { help(); return; }
  const started = performance.now();
  const cold = await measureCold(args);
  const warm = await measureWarm(args);
  const checks = [
    check('cold-stdio-fixture-measured', cold.failures === 0 && cold.stats.totalMs.count > 0, 'high', `failures=${cold.failures}, p95=${cold.stats.totalMs.p95}ms`, 'Fix fixture/client lifecycle before trusting overhead numbers.'),
    check('warm-tools-list-measured', warm.failures === 0 && warm.stats.toolsListMs.count > 0, 'high', `failures=${warm.failures}, p95=${warm.stats.toolsListMs.p95}ms`, 'Warm discovery must be stable before adding larger benchmarks.'),
    check('cold-stdio-not-paid-per-request-budget', (cold.stats.totalMs.p95 || 0) < 2000, 'medium', `cold p95=${cold.stats.totalMs.p95}ms`, 'Use cache/reuse rather than cold start per call.'),
    check('warm-tools-list-budget', (warm.stats.toolsListMs.p95 || 0) < 50, 'medium', `warm p95=${warm.stats.toolsListMs.p95}ms`, 'Keep warm `tools/list` on a hot session cheap.'),
  ];
  const blockers = checks.filter((row) => !row.ok && row.severity === 'high').map((row) => `${row.id}: ${row.evidence}`);
  const warnings = checks.filter((row) => !row.ok && row.severity !== 'high').map((row) => `${row.id}: ${row.evidence}`);
  const report = { schema: 'mcpace.mcpFixtureOverhead.v1', status: blockers.length ? 'fail' : 'pass', generatedAt: new Date().toISOString(), project: { name: deriveProjectName(), version: deriveProjectVersion() }, fixture: FIXTURE, config: { coldRuns: args.coldRuns, warmLists: args.warmLists, timeoutMs: args.timeoutMs }, safety: { startsThirdPartyMcpServers: false, callsThirdPartyTools: false, packageInstallScriptsAllowed: false, fixtureOnly: true }, cold, warm, checks, blockers, warnings, elapsedMs: Number((performance.now() - started).toFixed(3)) };
  if (args.write) { fs.mkdirSync(path.dirname(args.write), { recursive: true }); fs.writeFileSync(args.write, `${JSON.stringify(report, null, 2)}\n`); }
  if (args.markdown) { fs.mkdirSync(path.dirname(args.markdown), { recursive: true }); fs.writeFileSync(args.markdown, markdown(report)); }
  if (args.json) console.log(JSON.stringify(report, null, 2));
  else process.stdout.write(markdown(report));
  if (report.status !== 'pass') process.exitCode = 1;
}

main().catch((error) => {
  console.error(error.stack || error.message);
  process.exitCode = 1;
});
