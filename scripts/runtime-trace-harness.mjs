#!/usr/bin/env node
import fs from 'node:fs';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { spawn } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import { deriveProjectName, deriveProjectVersion, repoRoot } from './lib/project-metadata.mjs';
import { cleanChildEnv } from './lib/safe-child-env.mjs';
import { binaryNameForPlatform, binaryNameForTarget, currentTargetKey, detectTarget } from '../packages/npm/cli/lib/platform.js';

const DEFAULT_ENDPOINT_PATH = '/mcp';
const DEFAULT_TIMEOUT_MS = 7_000;

function parseArgs(argv) {
  const parsed = { json: false, write: null, markdown: null, binary: null, endpoint: null, strict: false, help: false };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--json': parsed.json = true; break;
      case '--write': parsed.write = argv[++index] || null; if (!parsed.write) throw new Error('runtime-trace-harness requires a path after --write'); break;
      case '--markdown':
      case '--write-md': parsed.markdown = argv[++index] || null; if (!parsed.markdown) throw new Error('runtime-trace-harness requires a path after --markdown'); break;
      case '--binary': parsed.binary = argv[++index] || null; if (!parsed.binary) throw new Error('runtime-trace-harness requires a path after --binary'); break;
      case '--endpoint': parsed.endpoint = argv[++index] || null; if (!parsed.endpoint) throw new Error('runtime-trace-harness requires a URL after --endpoint'); break;
      case '--strict': parsed.strict = true; break;
      case '-h':
      case '--help': parsed.help = true; break;
      default: throw new Error(`unsupported runtime-trace-harness argument: ${token}`);
    }
  }
  return parsed;
}

function printHelp() {
  process.stdout.write('Usage: node scripts/runtime-trace-harness.mjs [--json] [--write <path>] [--markdown <path>] [--binary <path>] [--endpoint <url>] [--strict]\n\nRuns the local runtime proof when a binary is present: client -> /mcp -> initialize -> tools/list -> tools/call -> tiny stdio upstream. If prerequisites are missing, records an explicit blocked report.\n');
}

function exists(relativePath) {
  return fs.existsSync(path.join(repoRoot, relativePath));
}

function uniquePaths(paths) {
  const seen = new Set();
  return paths.filter((candidate) => {
    if (!candidate) return false;
    const normalized = path.normalize(candidate);
    if (seen.has(normalized)) return false;
    seen.add(normalized);
    return true;
  });
}

function isExecutable(filePath) {
  if (process.platform === 'win32') return true;
  try {
    fs.accessSync(filePath, fs.constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

function currentHostTarget() {
  const target = detectTarget();
  return {
    key: target?.key ?? currentTargetKey(),
    detected: Boolean(target),
    platform: process.platform,
    arch: process.arch,
    libc: target?.libcProbe ?? null,
    rustTarget: target?.rustTarget ?? null,
  };
}

function defaultBinaryCandidates() {
  const exe = binaryNameForPlatform();
  const target = detectTarget();
  const targetKey = target?.key ?? currentTargetKey();
  const targetBinary = binaryNameForTarget(target || { platform: process.platform });
  return uniquePaths([
    process.env.MCPACE_BINARY_PATH ? path.resolve(process.env.MCPACE_BINARY_PATH) : null,
    process.env.MCPACE_DEV_BINARY ? path.resolve(process.env.MCPACE_DEV_BINARY) : null,
    path.join(repoRoot, 'target', 'release', exe),
    path.join(repoRoot, 'target', 'debug', exe),
    target?.rustTarget ? path.join(repoRoot, 'target', target.rustTarget, 'release', targetBinary) : null,
    path.join(repoRoot, 'dist', exe),
    path.join(repoRoot, 'packages', 'npm', 'cli', 'vendor', targetKey, targetBinary),
    path.join(repoRoot, 'packages', 'npm', 'cli', 'vendor', exe),
  ]);
}

function presentBinaryFromCandidates(candidates) {
  return candidates.find((candidate) => fs.existsSync(candidate) && isExecutable(candidate)) || null;
}

function step(id, required, status, evidence) {
  return { id, required, status, evidence };
}

function makeBaseReport(options) {
  const candidates = options.binary ? [path.resolve(options.binary)] : defaultBinaryCandidates();
  const presentBinary = presentBinaryFromCandidates(candidates);
  const host = currentHostTarget();
  const endpoint = options.endpoint || 'http://127.0.0.1:39022/mcp';
  const tinyFixturePath = [
    'tests/fixtures/tiny-mcp-stdio-server.mjs',
    'tests/fixtures/tiny-stdio-mcp-server.mjs',
    'examples/tiny-stdio-mcp-server.mjs',
  ].find(exists) || null;
  const steps = [
    step('binary', true, presentBinary ? 'pass' : 'blocked', presentBinary || candidates.map((candidate) => path.relative(repoRoot, candidate).split(path.sep).join('/')).join(', ')),
    step('tiny-upstream-fixture', true, tinyFixturePath ? 'pass' : 'blocked', tinyFixturePath || 'tests/fixtures/tiny-mcp-stdio-server.mjs'),
    step('serve-endpoint', true, 'pending', endpoint),
    step('initialize', true, 'pending', 'POST JSON-RPC initialize with Accept: application/json, text/event-stream'),
    step('tools-list', true, 'pending', 'POST JSON-RPC tools/list through MCPace'),
    step('upstream-call', true, 'pending', 'POST JSON-RPC tools/call -> upstream_call against tiny stdio server'),
  ];
  const blockers = steps.filter((step) => step.status === 'blocked').map((step) => `${step.id}: ${step.evidence}`);
  return {
    schema: 'mcpace.runtimeTraceHarness.v1',
    generatedAt: new Date().toISOString(),
    project: { name: deriveProjectName(), version: deriveProjectVersion() },
    status: blockers.length === 0 ? 'running' : 'blocked',
    endpoint,
    binary: presentBinary ? path.relative(repoRoot, presentBinary).split(path.sep).join('/') : null,
    host,
    binaryCandidates: candidates.map((candidate) => path.relative(repoRoot, candidate).split(path.sep).join('/')),
    mode: options.endpoint ? 'external-endpoint' : 'spawned-local-serve',
    blockers,
    failures: [],
    trace: null,
    steps,
    nextCommands: [
      'cargo build --release --locked',
      'node scripts/runtime-trace-harness.mjs --json --write reports/runtime-trace-latest.json --markdown reports/runtime-trace-latest.md',
      './target/release/mcpace serve --port 39022',
      'Run initialize -> tools/list -> tools/call/upstream_call through /mcp and record reports/runtime-trace-latest.json.',
    ],
    _internal: {
      binaryPath: presentBinary,
      tinyFixturePath: tinyFixturePath ? path.join(repoRoot, tinyFixturePath) : null,
    },
  };
}

function withoutInternal(report) {
  const copy = { ...report };
  delete copy._internal;
  return copy;
}

function markStep(report, id, status, evidence) {
  const found = report.steps.find((entry) => entry.id === id);
  if (!found) throw new Error(`unknown runtime trace step: ${id}`);
  found.status = status;
  found.evidence = evidence;
}

function jsonEscape(value) {
  return JSON.stringify(value);
}

function writeTraceRoot(rootPath, fixturePath) {
  fs.writeFileSync(
    path.join(rootPath, 'mcpace.config.json'),
    `${JSON.stringify({
      version: deriveProjectVersion(),
      client: { keyName: 'MCPace' },
      profiles: {
        runtime: {
          default: 'safe',
          profiles: {
            safe: { description: 'Safe runtime trace profile', serverOverrides: {} },
          },
        },
      },
      servers: {
        tiny: {
          kind: 'host-stdio',
          required: true,
          transportPreference: 'stdio',
          policy: {
            scopeClass: 'shared-global',
            concurrencyPolicy: 'single-writer',
            stateBinding: 'none',
            credentialBinding: 'none',
            parallelismLimit: 1,
            conflictDomain: 'tiny-runtime-trace',
          },
          installer: {
            installTarget: 'none',
            installMethod: 'none',
            installPackage: '',
            verifyCommand: '',
          },
        },
      },
    }, null, 2)}\n`,
    'utf8',
  );
  fs.writeFileSync(
    path.join(rootPath, 'mcp_settings.json'),
    `{
  "mcpServers": {
    "tiny": {
      "enabled": true,
      "type": "stdio",
      "command": ${jsonEscape(process.execPath)},
      "args": [${jsonEscape(fixturePath)}]
    }
  }
}
`,
    'utf8',
  );
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function withTimeout(promise, timeoutMs, label) {
  return Promise.race([
    promise,
    new Promise((_, reject) => setTimeout(() => reject(new Error(`${label} timed out after ${timeoutMs}ms`)), timeoutMs)),
  ]);
}

function reserveLocalPort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.on('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const address = server.address();
      const port = typeof address === 'object' && address ? address.port : null;
      server.close(() => {
        if (port) resolve(port);
        else reject(new Error('failed to reserve a local port'));
      });
    });
  });
}

async function waitForHealth(baseUrl, childOutput) {
  for (let attempt = 0; attempt < 50; attempt += 1) {
    try {
      const response = await fetch(`${baseUrl}/healthz`);
      if (response.ok) return;
    } catch {
      // Retry until the spawned listener is ready.
    }
    await sleep(100);
  }
  throw new Error(`serve endpoint did not become healthy; stdout=${childOutput.stdout.slice(-500)} stderr=${childOutput.stderr.slice(-500)}`);
}

function parseJsonRpcResponse(text) {
  const trimmed = text.trim();
  if (!trimmed) throw new Error('empty JSON-RPC response body');
  if (trimmed.startsWith('{')) return JSON.parse(trimmed);
  const dataLines = trimmed
    .split(/\r?\n/)
    .filter((line) => line.startsWith('data:'))
    .map((line) => line.slice('data:'.length).trim())
    .filter(Boolean);
  if (dataLines.length > 0) return JSON.parse(dataLines.join('\n'));
  throw new Error(`unsupported JSON-RPC response body: ${trimmed.slice(0, 200)}`);
}

async function postJsonRpc(endpoint, request, extraHeaders = {}) {
  const response = await withTimeout(fetch(endpoint, {
    method: 'POST',
    headers: {
      Accept: 'application/json, text/event-stream',
      'Content-Type': 'application/json',
      'Mcp-Method': request.method,
      ...extraHeaders,
    },
    body: JSON.stringify(request),
  }), DEFAULT_TIMEOUT_MS, `${request.method} HTTP request`);
  const bodyText = await response.text();
  const body = parseJsonRpcResponse(bodyText);
  if (!response.ok) {
    throw new Error(`${request.method} returned HTTP ${response.status}: ${bodyText.slice(0, 500)}`);
  }
  if (body.error) {
    throw new Error(`${request.method} returned JSON-RPC error: ${JSON.stringify(body.error)}`);
  }
  return { response, body, bodyText };
}

async function runRuntimeTrace(report) {
  if (report.blockers.length > 0) return report;

  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-runtime-trace-'));
  const childOutput = { stdout: '', stderr: '' };
  let child = null;
  let endpoint = report.endpoint;
  let cleanupRoot = true;

  try {
    if (!report._internal.binaryPath) throw new Error('missing binary path after prerequisite check');
    if (!report._internal.tinyFixturePath) throw new Error('missing tiny fixture path after prerequisite check');

    if (!endpoint || endpoint === 'http://127.0.0.1:39022/mcp') {
      const port = await reserveLocalPort();
      const baseUrl = `http://127.0.0.1:${port}`;
      endpoint = `${baseUrl}${DEFAULT_ENDPOINT_PATH}`;
      report.endpoint = endpoint;
      writeTraceRoot(tempRoot, report._internal.tinyFixturePath);
      child = spawn(report._internal.binaryPath, [
        'serve',
        '--root',
        tempRoot,
        '--host',
        '127.0.0.1',
        '--port',
        String(port),
        '--max-connections',
        '8',
      ], {
        cwd: repoRoot,
        env: cleanChildEnv({ MCPACE_STATE_ROOT: tempRoot }),
        stdio: ['ignore', 'pipe', 'pipe'],
        windowsHide: true,
      });
      child.stdout?.on('data', (chunk) => { childOutput.stdout += chunk; });
      child.stderr?.on('data', (chunk) => { childOutput.stderr += chunk; });
      child.on('exit', (code, signal) => {
        childOutput.exit = { code, signal };
      });
      await waitForHealth(baseUrl, childOutput);
      markStep(report, 'serve-endpoint', 'pass', `${endpoint} (spawned from ${report.binary})`);
    } else {
      cleanupRoot = false;
      markStep(report, 'serve-endpoint', 'pass', `${endpoint} (external endpoint supplied)`);
    }

    const initialize = await postJsonRpc(endpoint, {
      jsonrpc: '2.0',
      id: 1,
      method: 'initialize',
      params: {
        protocolVersion: '2025-11-25',
        capabilities: {},
        clientInfo: { name: 'mcpace-runtime-trace-harness', version: '0.1.0' },
      },
    });
    const sessionId = initialize.response.headers.get('mcp-session-id') || null;
    const protocolVersion = initialize.response.headers.get('mcp-protocol-version') || initialize.body.result?.protocolVersion || null;
    markStep(report, 'initialize', 'pass', `protocol=${protocolVersion || '<unspecified>'}; session=${sessionId || '<none>'}`);

    const sessionHeaders = sessionId ? { 'Mcp-Session-Id': sessionId } : {};
    const toolsList = await postJsonRpc(endpoint, {
      jsonrpc: '2.0',
      id: 2,
      method: 'tools/list',
      params: {},
    }, sessionHeaders);
    const toolNames = toolsList.body.result?.tools?.map((tool) => tool.name).filter(Boolean) || [];
    if (!toolNames.includes('upstream_call')) throw new Error('tools/list did not advertise upstream_call');
    markStep(report, 'tools-list', 'pass', `${toolNames.length} tools; upstream_call advertised`);

    const upstreamCall = await postJsonRpc(endpoint, {
      jsonrpc: '2.0',
      id: 3,
      method: 'tools/call',
      params: {
        name: 'upstream_call',
        arguments: {
          server: 'tiny',
          tool: 'tiny_echo',
          arguments: { message: 'trace-ok' },
          timeoutMs: 5_000,
          diagnostics: 'summary',
          sessionId: 'runtime-trace-smoke',
        },
      },
    }, { ...sessionHeaders, 'Mcp-Name': 'upstream_call' });
    const structured = upstreamCall.body.result?.structuredContent || {};
    const text = upstreamCall.body.result?.content?.find((entry) => entry.type === 'text')?.text || '';
    if (structured.upstreamOk !== true || structured.leaseReleased !== true || !text.includes('tiny_echo:trace-ok')) {
      throw new Error(`upstream_call trace did not prove echo+lease release: ${JSON.stringify({ structured, text }).slice(0, 800)}`);
    }
    markStep(report, 'upstream-call', 'pass', `tiny_echo returned "${text}"; leaseReleased=${structured.leaseReleased}`);

    report.trace = {
      endpoint,
      sessionId,
      protocolVersion,
      topLevelToolCount: toolNames.length,
      upstreamCallAdvertised: toolNames.includes('upstream_call'),
      projectedTinyToolAdvertised: toolNames.some((name) => /^u_tiny_tiny_echo_/.test(name)),
      upstream: {
        server: structured.server,
        tool: structured.tool,
        text,
        upstreamOk: structured.upstreamOk,
        leaseAttached: structured.leaseAttached,
        leaseReleased: structured.leaseReleased,
        sessionPoolEnabled: structured.sessionPoolEnabled,
        sessionPoolReused: structured.sessionPoolReused,
      },
      childProcess: {
        stdoutTail: childOutput.stdout.slice(-500),
        stderrTail: childOutput.stderr.slice(-500),
      },
      cleanup: cleanupRoot ? 'temporary trace root removed after run' : 'external endpoint supplied; no temporary root cleanup',
    };
    report.status = 'pass';
    report.nextCommands = [
      'npm run verify:product-practice',
      'Stage and verify at least one native binary/platform package before claiming published install readiness.',
      'Keep durable HTTP session storage and remote HTTP upstream forwarding as separate future runtime hardening work.',
    ];
    return report;
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    report.status = 'fail';
    report.failures.push(message);
    for (const pending of report.steps.filter((entry) => entry.status === 'pending')) {
      pending.status = 'blocked';
      pending.evidence = `not reached after failure: ${message}`;
    }
    report.nextCommands = [
      'cargo build --release --locked',
      'node scripts/runtime-trace-harness.mjs --json --write reports/runtime-trace-latest.json --markdown reports/runtime-trace-latest.md',
      'Inspect the failing runtime trace step and fix the broker loop before adding more feature surface.',
    ];
    return report;
  } finally {
    if (child?.pid) {
      try { child.kill('SIGTERM'); } catch { /* ignore cleanup errors */ }
      setTimeout(() => {
        try { child.kill('SIGKILL'); } catch { /* ignore cleanup errors */ }
      }, 2_000).unref();
    }
    if (cleanupRoot) {
      try { fs.rmSync(tempRoot, { recursive: true, force: true }); } catch { /* ignore cleanup errors */ }
    }
  }
}

function renderMarkdown(report) {
  const lines = ['# MCPace runtime trace harness', '', `Project: \`${report.project.name}\` v\`${report.project.version}\``, `Status: \`${report.status}\``, '', '## Steps', '', '| step | status | evidence |', '|---|---:|---|'];
  for (const step of report.steps) lines.push(`| ${step.id} | ${step.status} | ${String(step.evidence).replace(/\|/g, '\\|')} |`);
  if (report.blockers.length > 0) {
    lines.push('', '## Blockers', '');
    for (const blocker of report.blockers) lines.push(`- ${blocker}`);
  }
  if (report.failures?.length > 0) {
    lines.push('', '## Failures', '');
    for (const failure of report.failures) lines.push(`- ${failure}`);
  }
  if (report.trace) {
    lines.push('', '## Trace evidence', '');
    lines.push(`- endpoint: \`${report.trace.endpoint}\``);
    lines.push(`- session: \`${report.trace.sessionId || '<none>'}\``);
    lines.push(`- top-level tools: \`${report.trace.topLevelToolCount}\``);
    lines.push(`- upstream: \`${report.trace.upstream.server}/${report.trace.upstream.tool}\` -> \`${report.trace.upstream.text}\``);
    lines.push(`- lease: attached=\`${report.trace.upstream.leaseAttached}\`, released=\`${report.trace.upstream.leaseReleased}\``);
  }
  lines.push('', '## Next commands', '');
  for (const command of report.nextCommands) lines.push(`- \`${command}\``);
  lines.push('');
  return lines.join('\n');
}

function writeFileEnsuringDir(filePath, contents) {
  const target = path.resolve(repoRoot, filePath);
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, contents, 'utf8');
}

function isCliInvocation() {
  const entry = process.argv[1];
  return entry ? pathToFileURL(path.resolve(entry)).href === import.meta.url : false;
}

async function main() {
  try {
    const parsed = parseArgs(process.argv.slice(2));
    if (parsed.help) { printHelp(); return; }
    const report = withoutInternal(await runRuntimeTrace(makeBaseReport(parsed)));
    if (parsed.write) writeFileEnsuringDir(parsed.write, `${JSON.stringify(report, null, 2)}\n`);
    if (parsed.markdown) writeFileEnsuringDir(parsed.markdown, renderMarkdown(report));
    if (parsed.json) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
    else process.stdout.write(`[mcpace runtime-trace] ${report.status}\n`);
    if (parsed.strict && report.status !== 'pass') process.exit(1);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exit(1);
  }
}

if (isCliInvocation()) main();
