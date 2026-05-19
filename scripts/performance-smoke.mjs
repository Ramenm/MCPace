#!/usr/bin/env node
import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { spawn, spawnSync } from 'node:child_process';
import { performance } from 'node:perf_hooks';
import { cleanChildEnv } from './lib/safe-child-env.mjs';
import { repoRoot } from './lib/project-metadata.mjs';

function parseArgs(argv) {
  const args = {
    json: false,
    write: 'reports/performance-smoke-latest.json',
    markdown: 'reports/performance-smoke-latest.md',
    requests: 80,
    concurrency: 8,
    timeoutMs: 5_000,
    servers: 20,
    tools: 50_000,
    memoryLimitMiB: 256,
    maxHttpP95Ms: null,
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
      case '--write': args.write = readValue(); break;
      case '--markdown': args.markdown = readValue(); break;
      case '--no-write': args.write = null; args.markdown = null; break;
      case '--requests': args.requests = parsePositiveInteger(readValue(), token); break;
      case '--concurrency': args.concurrency = parsePositiveInteger(readValue(), token); break;
      case '--timeout-ms': args.timeoutMs = parsePositiveInteger(readValue(), token); break;
      case '--servers': args.servers = parsePositiveInteger(readValue(), token); break;
      case '--tools': args.tools = parsePositiveInteger(readValue(), token); break;
      case '--memory-limit-mib': args.memoryLimitMiB = parsePositiveInteger(readValue(), token); break;
      case '--max-http-p95-ms': args.maxHttpP95Ms = parsePositiveNumber(readValue(), token); break;
      case '--help':
      case '-h':
        args.help = true;
        break;
      default:
        throw new Error(`unsupported performance-smoke argument: ${token}`);
    }
  }

  return args;
}

function parsePositiveInteger(value, label) {
  const parsed = Number.parseInt(value, 10);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) throw new Error(`${label} must be a positive integer`);
  return parsed;
}

function parsePositiveNumber(value, label) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed) || parsed <= 0) throw new Error(`${label} must be a positive number`);
  return parsed;
}

function printHelp() {
  console.log(`Usage: npm run verify:performance -- [options]

Runs a bounded performance smoke suite without external dependencies:
  1. starts a local mock HTTP endpoint and measures benchmark:runtime output;
  2. runs synthetic tool-scale, mixed-upstream, and upstream-failsafe simulations;
  3. writes JSON and Markdown reports for release evidence.

Options:
  --requests 80                 HTTP requests per mock path
  --concurrency 8               HTTP benchmark concurrency
  --servers 20                  Synthetic upstream/server count
  --tools 50000                 Synthetic tool count
  --memory-limit-mib 256        Synthetic heap budget
  --max-http-p95-ms <number>    Optional host-specific p95 gate
  --write <path>                JSON report path
  --markdown <path>             Markdown report path
  --no-write                    Print only
  --json                        Print machine-readable JSON
`);
}

async function withMockServer(callback) {
  const server = http.createServer((request, response) => {
    const url = new URL(request.url || '/', 'http://127.0.0.1');
    response.setHeader('content-type', 'application/json; charset=utf-8');
    response.setHeader('cache-control', 'no-store');

    if (url.pathname === '/healthz') {
      response.end(JSON.stringify({
        ok: true,
        readiness: 'mock-ready',
        runtime: {
          http: { activeConnections: 1, maxConnections: 8 },
          upstreamSessionPool: { shards: 4, sessions: 0 },
        },
      }));
      return;
    }

    if (url.pathname === '/api/resources') {
      response.end(JSON.stringify({
        ok: true,
        resources: {
          maxHttpHeaderCount: 96,
          defaultHttpConnectionLimit: 8,
          availableParallelism: os.availableParallelism?.() || os.cpus().length || 1,
        },
      }));
      return;
    }

    response.statusCode = 404;
    response.end(JSON.stringify({ ok: false, error: 'not-found' }));
  });

  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => resolve());
  });

  const address = server.address();
  const baseUrl = `http://127.0.0.1:${address.port}`;
  try {
    return await callback(baseUrl);
  } finally {
    await new Promise((resolve) => server.close(() => resolve()));
  }
}


function runNodeScriptAsync(script, args) {
  const started = performance.now();
  return new Promise((resolve) => {
    const child = spawn(process.execPath, [script, ...args], {
      cwd: repoRoot,
      env: cleanChildEnv(),
      windowsHide: true,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    let stdout = '';
    let stderr = '';
    const killTimer = setTimeout(() => child.kill('SIGTERM'), 120_000);
    child.stdout.on('data', (chunk) => { stdout += chunk; });
    child.stderr.on('data', (chunk) => { stderr += chunk; });
    child.on('error', (error) => {
      clearTimeout(killTimer);
      resolve({ status: null, signal: null, durationMs: Number((performance.now() - started).toFixed(2)), stdout: '', stderr: error.message, parsed: null });
    });
    child.on('close', (status, signal) => {
      clearTimeout(killTimer);
      stdout = String(stdout || '').trim();
      stderr = String(stderr || '').trim();
      let parsed = null;
      if (stdout) {
        try {
          parsed = JSON.parse(stdout);
        } catch {
          const lastJsonLine = stdout.split(/\r?\n/).reverse().find((line) => line.trim().startsWith('{'));
          if (lastJsonLine) parsed = JSON.parse(lastJsonLine);
        }
      }
      resolve({
        status,
        signal,
        durationMs: Number((performance.now() - started).toFixed(2)),
        stdout: stdout.slice(0, 12_000),
        stderr: stderr.slice(0, 12_000),
        parsed,
      });
    });
  });
}

function runNodeScript(script, args) {
  const started = performance.now();
  const result = spawnSync(process.execPath, [script, ...args], {
    cwd: repoRoot,
    encoding: 'utf8',
    env: cleanChildEnv(),
    timeout: 120_000,
    windowsHide: true,
  });
  const durationMs = Number((performance.now() - started).toFixed(2));
  const stdout = String(result.stdout || '').trim();
  const stderr = String(result.stderr || '').trim();
  let parsed = null;
  if (stdout) {
    try {
      parsed = JSON.parse(stdout);
    } catch {
      const lastJsonLine = stdout.split(/\r?\n/).reverse().find((line) => line.trim().startsWith('{'));
      if (lastJsonLine) parsed = JSON.parse(lastJsonLine);
    }
  }
  return {
    status: result.status,
    signal: result.signal,
    durationMs,
    stdout: stdout.slice(0, 12_000),
    stderr: stderr.slice(0, 12_000),
    parsed,
  };
}

async function runHttpBenchmark(args) {
  return withMockServer(async (baseUrl) => {
    const run = await runNodeScriptAsync('scripts/benchmark-runtime.mjs', [
      '--url', baseUrl,
      '--paths', '/healthz,/api/resources',
      '--requests', String(args.requests),
      '--concurrency', String(args.concurrency),
      '--timeout-ms', String(args.timeoutMs),
      '--json',
    ]);
    return { baseUrl, run };
  });
}

function runSyntheticBenchmarks(args) {
  const common = [
    '--servers', String(args.servers),
    '--tools', String(args.tools),
    '--memory-limit-mib', String(args.memoryLimitMiB),
    '--json',
  ];
  return {
    toolScale: runNodeScript('scripts/simulate-tool-scale.mjs', common),
    mixedUpstreams: runNodeScript('scripts/simulate-mixed-upstreams.mjs', common),
    upstreamFailsafe: runNodeScript('scripts/simulate-upstream-failsafe.mjs', [...common, '--retries', '1']),
  };
}

function finiteNumber(value) {
  return typeof value === 'number' && Number.isFinite(value);
}

function evaluateReport({ args, httpBenchmark, synthetic }) {
  const checks = [];
  const httpReport = httpBenchmark.run.parsed;
  checks.push({
    id: 'runtime-http-benchmark-ran',
    ok: httpBenchmark.run.status === 0 && Boolean(httpReport),
    detail: `exit=${httpBenchmark.run.status}`,
  });

  const httpResults = Array.isArray(httpReport?.results) ? httpReport.results : [];
  const httpFailures = httpResults.reduce((total, result) => total + Number(result.failureCount || 0), 0);
  const httpP95Values = httpResults.map((result) => result.latencyMs?.p95).filter(finiteNumber);
  const maxHttpP95Ms = httpP95Values.length ? Math.max(...httpP95Values) : null;
  checks.push({ id: 'runtime-http-no-failures', ok: httpFailures === 0 && httpResults.length > 0, detail: `failures=${httpFailures}` });
  checks.push({ id: 'runtime-http-latency-measured', ok: httpP95Values.length === httpResults.length && httpP95Values.length > 0, detail: `maxP95Ms=${maxHttpP95Ms ?? 'n/a'}` });
  if (args.maxHttpP95Ms !== null) {
    checks.push({ id: 'runtime-http-p95-budget', ok: maxHttpP95Ms !== null && maxHttpP95Ms <= args.maxHttpP95Ms, detail: `maxP95Ms=${maxHttpP95Ms}; budget=${args.maxHttpP95Ms}` });
  }

  for (const [id, run] of Object.entries(synthetic)) {
    const parsed = run.parsed;
    checks.push({ id: `${id}-ran`, ok: run.status === 0 && Boolean(parsed), detail: `exit=${run.status}` });
    checks.push({ id: `${id}-status-pass`, ok: parsed?.status === 'pass', detail: `status=${parsed?.status ?? 'missing'}` });
    const heap = parsed?.budgets?.heapDeltaMiB;
    const limit = parsed?.budgets?.memoryLimitMiB ?? args.memoryLimitMiB;
    checks.push({ id: `${id}-heap-budget`, ok: finiteNumber(heap) && heap <= limit, detail: `heapDeltaMiB=${heap ?? 'n/a'}; limit=${limit}` });
  }

  return {
    status: checks.every((check) => check.ok) ? 'pass' : 'fail',
    checks,
    summary: {
      runtimeHttpMaxP95Ms: maxHttpP95Ms,
      runtimeHttpFailures: httpFailures,
      toolScaleElapsedMs: synthetic.toolScale.parsed?.elapsedMs ?? null,
      mixedUpstreamsElapsedMs: synthetic.mixedUpstreams.parsed?.elapsedMs ?? null,
      upstreamFailsafeElapsedMs: synthetic.upstreamFailsafe.parsed?.elapsedMs ?? null,
      toolScaleHeapDeltaMiB: synthetic.toolScale.parsed?.budgets?.heapDeltaMiB ?? null,
      mixedUpstreamsHeapDeltaMiB: synthetic.mixedUpstreams.parsed?.budgets?.heapDeltaMiB ?? null,
      upstreamFailsafeHeapDeltaMiB: synthetic.upstreamFailsafe.parsed?.budgets?.heapDeltaMiB ?? null,
    },
  };
}

function markdownReport(report) {
  const lines = [];
  lines.push(`# Performance smoke report`);
  lines.push('');
  lines.push(`Generated: ${report.generatedAt}`);
  lines.push(`Status: **${report.status}**`);
  lines.push('');
  lines.push('## Scope');
  lines.push('');
  lines.push('- Lightweight runtime HTTP benchmark against an in-process mock endpoint using `scripts/benchmark-runtime.mjs`.');
  lines.push('- Synthetic tool-scale, mixed-upstream, and upstream-failsafe simulations using bounded memory budgets.');
  lines.push('- This is a smoke/regression harness, not a replacement for host-specific Rust binary benchmarking.');
  lines.push('');
  lines.push('## Summary');
  lines.push('');
  lines.push(`- Runtime HTTP failures: ${report.summary.runtimeHttpFailures}`);
  lines.push(`- Runtime HTTP max p95: ${report.summary.runtimeHttpMaxP95Ms ?? 'n/a'} ms`);
  lines.push(`- Tool-scale: ${report.summary.toolScaleElapsedMs ?? 'n/a'} ms, heap +${report.summary.toolScaleHeapDeltaMiB ?? 'n/a'} MiB`);
  lines.push(`- Mixed-upstreams: ${report.summary.mixedUpstreamsElapsedMs ?? 'n/a'} ms, heap +${report.summary.mixedUpstreamsHeapDeltaMiB ?? 'n/a'} MiB`);
  lines.push(`- Upstream-failsafe: ${report.summary.upstreamFailsafeElapsedMs ?? 'n/a'} ms, heap +${report.summary.upstreamFailsafeHeapDeltaMiB ?? 'n/a'} MiB`);
  lines.push('');
  lines.push('## Checks');
  lines.push('');
  lines.push('| Check | Status | Detail |');
  lines.push('|---|---:|---|');
  for (const check of report.checks) {
    lines.push(`| ${check.id} | ${check.ok ? 'pass' : 'fail'} | ${String(check.detail).replace(/\|/g, '\\|')} |`);
  }
  lines.push('');
  lines.push('## Caveats');
  lines.push('');
  lines.push('- No `cargo`/`rustc` host proof is implied by this report.');
  lines.push('- Do not add hard latency gates until Ubuntu/macOS/Windows baselines exist. Use `--max-http-p95-ms` only after a baseline is accepted.');
  return `${lines.join('\n')}\n`;
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    printHelp();
    return;
  }

  const generatedAt = new Date().toISOString();
  const startedAt = performance.now();
  const httpBenchmark = await runHttpBenchmark(args);
  const synthetic = runSyntheticBenchmarks(args);
  const evaluation = evaluateReport({ args, httpBenchmark, synthetic });
  const report = {
    schema: 'mcpace.performanceSmoke.v1',
    status: evaluation.status,
    generatedAt,
    environment: {
      node: process.version,
      platform: process.platform,
      arch: process.arch,
      cpuCount: os.cpus().length,
      availableParallelism: os.availableParallelism?.() || os.cpus().length || null,
    },
    scenario: {
      requests: args.requests,
      concurrency: args.concurrency,
      timeoutMs: args.timeoutMs,
      servers: args.servers,
      tools: args.tools,
      memoryLimitMiB: args.memoryLimitMiB,
      maxHttpP95Ms: args.maxHttpP95Ms,
    },
    summary: evaluation.summary,
    checks: evaluation.checks,
    reports: {
      runtimeHttp: httpBenchmark.run.parsed,
      toolScale: synthetic.toolScale.parsed,
      mixedUpstreams: synthetic.mixedUpstreams.parsed,
      upstreamFailsafe: synthetic.upstreamFailsafe.parsed,
    },
    commandResults: {
      runtimeHttp: { status: httpBenchmark.run.status, durationMs: httpBenchmark.run.durationMs, stderr: httpBenchmark.run.stderr },
      toolScale: { status: synthetic.toolScale.status, durationMs: synthetic.toolScale.durationMs, stderr: synthetic.toolScale.stderr },
      mixedUpstreams: { status: synthetic.mixedUpstreams.status, durationMs: synthetic.mixedUpstreams.durationMs, stderr: synthetic.mixedUpstreams.stderr },
      upstreamFailsafe: { status: synthetic.upstreamFailsafe.status, durationMs: synthetic.upstreamFailsafe.durationMs, stderr: synthetic.upstreamFailsafe.stderr },
    },
    elapsedMs: Number((performance.now() - startedAt).toFixed(2)),
  };

  if (args.write) {
    const outputPath = path.resolve(repoRoot, args.write);
    fs.mkdirSync(path.dirname(outputPath), { recursive: true });
    fs.writeFileSync(outputPath, `${JSON.stringify(report, null, 2)}\n`);
  }
  if (args.markdown) {
    const markdownPath = path.resolve(repoRoot, args.markdown);
    fs.mkdirSync(path.dirname(markdownPath), { recursive: true });
    fs.writeFileSync(markdownPath, markdownReport(report));
  }

  if (args.json) {
    console.log(JSON.stringify(report, null, 2));
  } else {
    console.log(`${report.status}: performance smoke in ${report.elapsedMs}ms; runtime max p95=${report.summary.runtimeHttpMaxP95Ms ?? 'n/a'}ms`);
  }

  if (report.status !== 'pass') process.exitCode = 1;
}

main().catch((error) => {
  console.error(error.stack || error.message);
  process.exitCode = 1;
});
