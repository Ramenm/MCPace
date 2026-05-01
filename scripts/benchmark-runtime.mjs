#!/usr/bin/env node
import http from 'node:http';
import https from 'node:https';
import { performance } from 'node:perf_hooks';

function parseArgs(argv) {
  const args = {
    url: 'http://127.0.0.1:39022',
    paths: ['/healthz', '/api/resources'],
    requests: 50,
    concurrency: 8,
    timeoutMs: 5_000,
    json: false
  };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    const readValue = () => {
      const next = argv[index + 1];
      if (!next || next.startsWith('--')) {
        throw new Error(`${arg} requires a value`);
      }
      index += 1;
      return next;
    };
    if (arg === '--url') {
      args.url = readValue();
    } else if (arg === '--paths') {
      args.paths = readValue().split(',').map((value) => value.trim()).filter(Boolean);
    } else if (arg === '--requests') {
      args.requests = parsePositiveInteger(readValue(), '--requests');
    } else if (arg === '--concurrency') {
      args.concurrency = parsePositiveInteger(readValue(), '--concurrency');
    } else if (arg === '--timeout-ms') {
      args.timeoutMs = parsePositiveInteger(readValue(), '--timeout-ms');
    } else if (arg === '--json') {
      args.json = true;
    } else if (arg === '--help' || arg === '-h') {
      args.help = true;
    } else {
      throw new Error(`Unknown argument: ${arg}`);
    }
  }
  if (args.paths.length === 0) {
    throw new Error('--paths must include at least one comma-separated path');
  }
  return args;
}

function parsePositiveInteger(value, label) {
  const parsed = Number.parseInt(value, 10);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) {
    throw new Error(`${label} must be a positive integer`);
  }
  return parsed;
}

function percentile(sortedValues, percentileValue) {
  if (sortedValues.length === 0) {
    return null;
  }
  const index = Math.ceil((percentileValue / 100) * sortedValues.length) - 1;
  return sortedValues[Math.max(0, Math.min(sortedValues.length - 1, index))];
}

function requestOnce(targetUrl, timeoutMs) {
  const startedAt = performance.now();
  const transport = targetUrl.protocol === 'https:' ? https : http;
  return new Promise((resolve) => {
    const request = transport.request(
      targetUrl,
      {
        method: 'GET',
        timeout: timeoutMs,
        headers: {
          Connection: 'close',
          'User-Agent': 'mcpace-runtime-benchmark/1'
        }
      },
      (response) => {
        response.resume();
        response.on('end', () => {
          resolve({
            ok: response.statusCode >= 200 && response.statusCode < 500,
            statusCode: response.statusCode,
            latencyMs: performance.now() - startedAt
          });
        });
      }
    );

    request.on('timeout', () => {
      request.destroy(new Error(`request timed out after ${timeoutMs}ms`));
    });
    request.on('error', (error) => {
      resolve({ ok: false, error: error.message, latencyMs: performance.now() - startedAt });
    });
    request.end();
  });
}

async function benchmarkPath(baseUrl, path, requests, concurrency, timeoutMs) {
  const targetUrl = new URL(path, baseUrl);
  let launched = 0;
  let failureCount = 0;
  const latencies = [];
  const statusCounts = new Map();
  const errors = new Map();

  async function worker() {
    for (;;) {
      const current = launched;
      launched += 1;
      if (current >= requests) {
        return;
      }
      const result = await requestOnce(targetUrl, timeoutMs);
      latencies.push(result.latencyMs);
      if (result.statusCode !== undefined) {
        statusCounts.set(result.statusCode, (statusCounts.get(result.statusCode) || 0) + 1);
      }
      if (!result.ok) {
        failureCount += 1;
        const key = result.error || `HTTP ${result.statusCode}`;
        errors.set(key, (errors.get(key) || 0) + 1);
      }
    }
  }

  const workerCount = Math.min(concurrency, requests);
  const startedAt = performance.now();
  await Promise.all(Array.from({ length: workerCount }, () => worker()));
  const durationMs = performance.now() - startedAt;
  latencies.sort((left, right) => left - right);
  const sum = latencies.reduce((total, value) => total + value, 0);

  return {
    path,
    url: targetUrl.toString(),
    requests,
    concurrency: workerCount,
    durationMs: Number(durationMs.toFixed(2)),
    throughputPerSecond: Number((requests / Math.max(durationMs / 1000, 0.001)).toFixed(2)),
    failureCount,
    statusCounts: Object.fromEntries(statusCounts.entries()),
    errors: Object.fromEntries(errors.entries()),
    latencyMs: {
      min: Number((latencies[0] ?? 0).toFixed(2)),
      p50: Number((percentile(latencies, 50) ?? 0).toFixed(2)),
      p95: Number((percentile(latencies, 95) ?? 0).toFixed(2)),
      p99: Number((percentile(latencies, 99) ?? 0).toFixed(2)),
      max: Number((latencies.at(-1) ?? 0).toFixed(2)),
      avg: Number((sum / Math.max(latencies.length, 1)).toFixed(2))
    }
  };
}

function printHelp() {
  console.log(`Usage: npm run benchmark:runtime -- --url http://127.0.0.1:39022 [options]

Options:
  --paths /healthz,/api/resources  Comma-separated GET paths to measure
  --requests 50                    Requests per path
  --concurrency 8                  Concurrent requests per path
  --timeout-ms 5000                Per-request timeout
  --json                           Print machine-readable JSON
`);
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    printHelp();
    return;
  }
  const startedAt = new Date().toISOString();
  const results = [];
  for (const path of args.paths) {
    results.push(await benchmarkPath(args.url, path, args.requests, args.concurrency, args.timeoutMs));
  }
  const report = {
    ok: results.every((result) => result.failureCount === 0),
    generatedAt: startedAt,
    baseUrl: args.url,
    requestCountPerPath: args.requests,
    configuredConcurrency: args.concurrency,
    timeoutMs: args.timeoutMs,
    results
  };

  if (args.json) {
    console.log(JSON.stringify(report, null, 2));
    return;
  }

  for (const result of results) {
    console.log(`${result.path}: ${result.requests} requests, ${result.failureCount} failures, ${result.throughputPerSecond}/s`);
    console.log(`  latency ms: min=${result.latencyMs.min} p50=${result.latencyMs.p50} p95=${result.latencyMs.p95} p99=${result.latencyMs.p99} max=${result.latencyMs.max}`);
  }
  if (!report.ok) {
    process.exitCode = 1;
  }
}

main().catch((error) => {
  console.error(error.message);
  process.exitCode = 1;
});
