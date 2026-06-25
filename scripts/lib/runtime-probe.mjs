import { spawn } from 'node:child_process';
import { existsSync, statSync } from 'node:fs';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import http from 'node:http';
import net from 'node:net';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { performance } from 'node:perf_hooks';
import { cleanChildEnv } from './safe-child-env.mjs';
import { repoRoot } from './project-metadata.mjs';

export const MAX_HTTP_CONNECTIONS = 256;
export const MAX_HTTP_BODY_BYTES = 16 * 1024 * 1024;

export function positiveInteger(value, name) {
  const number = Number(value);
  if (!Number.isSafeInteger(number) || number <= 0) {
    throw new Error(`${name} must be a positive integer`);
  }
  return number;
}

export function nonnegativeInteger(value, name) {
  const number = Number(value);
  if (!Number.isSafeInteger(number) || number < 0) {
    throw new Error(`${name} must be a non-negative integer`);
  }
  return number;
}

export function boundedPositiveInteger(value, name, max) {
  const number = positiveInteger(value, name);
  if (number > max) throw new Error(`${name} must be <= ${max}`);
  return number;
}

export function round(value, places = 2) {
  const factor = 10 ** places;
  return Math.round(value * factor) / factor;
}

export function percentile(sortedValues, fraction) {
  if (!sortedValues.length) return 0;
  const index = Math.min(sortedValues.length - 1, Math.ceil(sortedValues.length * fraction) - 1);
  return sortedValues[Math.max(index, 0)];
}

export function latencySummary(values) {
  if (!values.length) return { min: 0, avg: 0, p50: 0, p95: 0, p99: 0, max: 0 };
  const sorted = [...values].sort((a, b) => a - b);
  const sum = sorted.reduce((acc, value) => acc + value, 0);
  return {
    min: round(sorted[0]),
    avg: round(sum / sorted.length),
    p50: round(percentile(sorted, 0.5)),
    p95: round(percentile(sorted, 0.95)),
    p99: round(percentile(sorted, 0.99)),
    max: round(sorted[sorted.length - 1]),
  };
}

export function unquotePath(value) {
  const trimmed = String(value || '').trim();
  if (trimmed.length >= 2) {
    const first = trimmed[0];
    const last = trimmed[trimmed.length - 1];
    if ((first === '"' && last === '"') || (first === "'" && last === "'")) return trimmed.slice(1, -1);
  }
  return trimmed;
}

export function explicitBinaryFromEnv() {
  for (const name of ['MCPACE_BINARY', 'MCPACE_BINARY_PATH', 'MCPACE_DEV_BINARY']) {
    const value = unquotePath(process.env[name]);
    if (value) return value;
  }
  return '';
}

export function binaryName() {
  return process.platform === 'win32' ? 'mcpace.exe' : 'mcpace';
}

export function defaultBinaryCandidates() {
  return [
    path.join(repoRoot, 'target', 'release', binaryName()),
    path.join(repoRoot, 'target', 'perf', binaryName()),
    path.join(repoRoot, 'target', 'debug', binaryName()),
  ];
}

export function defaultBinary() {
  const candidates = defaultBinaryCandidates();
  return candidates.find((candidate) => existsSync(candidate)) || candidates[0];
}

export function assertRunnableBinary(binaryPath) {
  let stat;
  try {
    stat = statSync(binaryPath);
  } catch {
    const defaults = defaultBinaryCandidates().map((candidate) => path.relative(repoRoot, candidate)).join(', ');
    throw new Error(
      `MCPace binary not found: ${binaryPath}. Build one with cargo build --release, pass --binary <path>, or set MCPACE_BINARY_PATH/MCPACE_DEV_BINARY. Checked defaults: ${defaults}.`,
    );
  }
  if (!stat.isFile()) throw new Error(`MCPace binary path is not a file: ${binaryPath}`);
  if (process.platform !== 'win32' && (stat.mode & 0o111) === 0) {
    throw new Error(`MCPace binary path is not executable: ${binaryPath}`);
  }
}

export async function makeIsolatedRoot(prefix = 'mcpace-probe-') {
  const root = await mkdtemp(path.join(tmpdir(), prefix));
  await mkdir(path.join(root, 'mcp_settings.d'), { recursive: true });
  await writeFile(
    path.join(root, 'mcpace.config.json'),
    `${JSON.stringify({
      name: 'mcpace-runtime-probe',
      version: '0.7.2',
      profiles: {
        runtime: {
          default: 'manual',
          profiles: { manual: { description: 'runtime probe empty profile', serverOverrides: {} } },
        },
      },
      serve: { host: '127.0.0.1', port: 0, mcpPath: '/mcp', publicUrl: '' },
      mcpSettings: { includeDirs: ['mcp_settings.d'], includePaths: [] },
    }, null, 2)}\n`,
  );
  await writeFile(path.join(root, 'mcp_settings.json'), '{"mcpServers":{}}\n');
  return root;
}

export async function reserveLoopbackPort() {
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

export async function startMcpaceServer(options = {}) {
  const binary = options.binary || defaultBinary();
  assertRunnableBinary(binary);
  const root = options.root || (await makeIsolatedRoot(options.rootPrefix || 'mcpace-probe-'));
  const selectedPort = options.port || (await reserveLoopbackPort());
  const args = [
    'serve',
    '--root',
    root,
    '--host',
    '127.0.0.1',
    '--port',
    String(selectedPort),
    '--max-connections',
    String(options.maxConnections || 64),
    '--max-body-bytes',
    String(options.maxBodyBytes || 65_536),
    '--overview-cache-ms',
    String(options.overviewCacheMs ?? 250),
  ];
  if (options.ioTimeoutMs) args.push('--io-timeout-ms', String(options.ioTimeoutMs));

  const child = spawn(binary, args, {
    env: cleanChildEnv({ MCPACE_TOOL_LIST_WARMUP: '0' }),
    stdio: ['ignore', 'pipe', 'pipe'],
    windowsHide: true,
  });

  let stdout = '';
  let stderr = '';
  const readyTimeoutMs = options.readyTimeoutMs || 10_000;
  const ready = new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`server did not become ready. stderr: ${stderr}`)), readyTimeoutMs);
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
    binary,
    root,
    port,
    child,
    get stdout() { return stdout; },
    get stderr() { return stderr; },
    stop: async () => {
      if (child.exitCode === null && !child.killed) {
        child.kill('SIGTERM');
        await new Promise((resolve) => child.once('exit', resolve));
      }
      if (!options.root) await rm(root, { recursive: true, force: true });
    },
  };
}

export function httpRequest({ port, method = 'GET', target, headers = {}, body = '', timeoutMs = 5_000 }) {
  const bodyBuffer = Buffer.from(body);
  const requestHeaders = { ...headers };
  if (bodyBuffer.length && !requestHeaders['Content-Length']) requestHeaders['Content-Length'] = String(bodyBuffer.length);
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
        let responseBody = '';
        response.setEncoding('utf8');
        response.on('data', (chunk) => { responseBody += chunk; });
        response.on('end', () => {
          resolve({
            ok: true,
            status: response.statusCode || 0,
            latencyMs: performance.now() - started,
            headers: response.headers,
            body: responseBody,
          });
        });
      },
    );
    request.setTimeout(timeoutMs, () => request.destroy(new Error('request timeout')));
    request.on('error', (error) => {
      resolve({ ok: false, error: error.message, status: 0, latencyMs: performance.now() - started, headers: {}, body: '' });
    });
    if (bodyBuffer.length) request.write(bodyBuffer);
    request.end();
  });
}

export async function jsonRequest(options) {
  const result = await httpRequest({ ...options, headers: { Accept: 'application/json', ...(options.headers || {}) } });
  if (!result.ok) return { ...result, jsonOk: false };
  try {
    return { ...result, jsonOk: true, payload: result.body ? JSON.parse(result.body) : null };
  } catch (error) {
    return { ...result, jsonOk: false, error: error?.message || String(error), raw: result.body.slice(0, 4096) };
  }
}

export function initializeBody(id = 1, clientName = 'mcpace-runtime-probe') {
  return JSON.stringify({
    jsonrpc: '2.0',
    id,
    method: 'initialize',
    params: {
      protocolVersion: '2025-06-18',
      capabilities: {},
      clientInfo: { name: clientName, version: '0.0.0' },
    },
  });
}

export function mcpHeaders(extra = {}) {
  return {
    Accept: 'application/json, text/event-stream',
    'Content-Type': 'application/json',
    ...extra,
  };
}

export async function mcpPost({ port, body, sessionId = '', protocolVersion = '', timeoutMs = 5_000 }) {
  const headers = mcpHeaders({
    ...(sessionId ? { 'Mcp-Session-Id': sessionId } : {}),
    ...(protocolVersion ? { 'MCP-Protocol-Version': protocolVersion } : {}),
  });
  return httpRequest({ port, method: 'POST', target: '/mcp', headers, body, timeoutMs });
}
