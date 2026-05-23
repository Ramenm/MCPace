#!/usr/bin/env node
import fs from 'node:fs';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { spawnSync } from 'node:child_process';
import { repoRoot, deriveProjectVersion } from './lib/project-metadata.mjs';

const args = new Set(process.argv.slice(2));
const jsonOutput = args.has('--json');
const skipRuntime = args.has('--skip-runtime');
const skipRelease = args.has('--skip-release');
const keepArtifacts = args.has('--keep-artifacts');

const checks = [];
const DEFAULT_COMMAND_TIMEOUT_MS = 180_000;
const RUNTIME_COMMAND_TIMEOUT_MS = 60_000;

function now() {
  return new Date().toISOString();
}

function durationMs(start) {
  return Date.now() - start;
}

function tail(value, limit = 6000) {
  const text = String(value || '');
  return text.length > limit ? `…${text.slice(-limit)}` : text;
}

function commandLine(command, commandArgs) {
  return [command, ...commandArgs].map((part) => (/[\s"'\\]/.test(part) ? JSON.stringify(part) : part)).join(' ');
}

function runCheck(agent, id, command, commandArgs, options = {}) {
  const started = Date.now();
  const result = spawnSync(command, commandArgs, {
    cwd: options.cwd ?? repoRoot,
    env: options.env ?? process.env,
    encoding: 'utf8',
    windowsHide: true,
    maxBuffer: options.maxBuffer ?? 64 * 1024 * 1024,
    timeout: options.timeoutMs ?? DEFAULT_COMMAND_TIMEOUT_MS,
  });

  const record = {
    agent,
    id,
    command: commandLine(command, commandArgs),
    cwd: options.cwd ?? repoRoot,
    status: result.status === 0 && !result.error ? 'pass' : 'failed',
    exitCode: result.status,
    signal: result.signal ?? null,
    durationMs: durationMs(started),
  };

  if (result.error) {
    record.error = result.error.message;
  }

  if (options.capture !== false) {
    record.stdoutTail = tail(result.stdout);
    record.stderrTail = tail(result.stderr);
  }

  checks.push(record);

  if (result.status !== 0 || result.error) {
    const detail = [result.error?.message, result.stderr, result.stdout].filter(Boolean).join('\n');
    throw new Error(`${agent}/${id} failed: ${record.command}\n${tail(detail, 12000)}`.trim());
  }
  return result;
}

function expect(condition, agent, id, message, extra = {}) {
  const record = {
    agent,
    id,
    command: 'internal assertion',
    cwd: repoRoot,
    status: condition ? 'pass' : 'failed',
    durationMs: 0,
    ...extra,
  };
  checks.push(record);
  if (!condition) {
    throw new Error(`${agent}/${id} failed: ${message}`);
  }
}

function npmBin(prefix, name) {
  return process.platform === 'win32'
    ? path.join(prefix, 'node_modules', '.bin', `${name}.cmd`)
    : path.join(prefix, 'node_modules', '.bin', name);
}

function getFreePort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const address = server.address();
      server.close(() => resolve(address.port));
    });
  });
}

function runNodeSmoke(agentTemp, binaryPath, expectedVersion) {
  const packDir = path.join(agentTemp, 'npm-pack');
  const installDir = path.join(agentTemp, 'npm-install smoke unicode пробелы');
  fs.mkdirSync(packDir, { recursive: true });
  fs.mkdirSync(installDir, { recursive: true });

  const pack = runCheck('packaging-agent', 'npm-pack-local-cli', 'npm', [
    'pack',
    '--workspace',
    '@mcpace/cli',
    '--json',
    '--pack-destination',
    packDir,
  ]);
  const packed = JSON.parse(pack.stdout)[0];
  const tarball = path.join(packDir, packed.filename);
  expect(fs.existsSync(tarball), 'packaging-agent', 'npm-pack-output-exists', `missing ${tarball}`, { tarball });

  runCheck('packaging-agent', 'npm-install-packed-cli', 'npm', [
    'install',
    '--prefix',
    installDir,
    '--ignore-scripts',
    '--no-audit',
    '--no-fund',
    tarball,
  ]);

  const shim = npmBin(installDir, 'mcpace');
  expect(fs.existsSync(shim), 'packaging-agent', 'npm-bin-created', `missing npm bin shim ${shim}`, { shim });

  const explicit = runCheck('packaging-agent', 'npm-shim-explicit-binary', shim, ['--version'], {
    env: { ...process.env, MCPACE_BINARY_PATH: binaryPath },
  });
  expect(explicit.stdout.trim() === expectedVersion, 'packaging-agent', 'npm-shim-explicit-version', 'explicit shim did not print expected version', {
    expectedVersion,
    actualVersion: explicit.stdout.trim(),
  });

  const fallbackPath = `${path.dirname(binaryPath)}${path.delimiter}${process.env.PATH || ''}`;
  const fallbackEnv = { ...process.env, PATH: fallbackPath };
  delete fallbackEnv.MCPACE_BINARY_PATH;
  const fallback = runCheck('packaging-agent', 'npm-shim-path-fallback', shim, ['--version'], {
    env: fallbackEnv,
  });
  expect(fallback.stdout.trim() === expectedVersion, 'packaging-agent', 'npm-shim-path-version', 'PATH fallback shim did not print expected version', {
    expectedVersion,
    actualVersion: fallback.stdout.trim(),
  });
}

async function runRuntimeSmoke(agentTemp, binaryPath) {
  if (skipRuntime) {
    checks.push({ agent: 'runtime-agent', id: 'runtime-smoke', command: 'skipped by --skip-runtime', cwd: repoRoot, status: 'skipped', durationMs: 0 });
    return;
  }

  const root = path.join(agentTemp, 'runtime root with spaces', 'юникод');
  fs.mkdirSync(root, { recursive: true });
  const port = await getFreePort();
  const common = ['--root', root, '--json'];

  try {
    const up = runCheck('runtime-agent', 'mcpace-up', binaryPath, [
      'up',
      '--root', root,
      '--client', 'none',
      '--no-server',
      '--port', String(port),
      '--json',
    ], { timeoutMs: RUNTIME_COMMAND_TIMEOUT_MS });
    const upJson = JSON.parse(up.stdout);
    expect(upJson.ok === true || upJson.status === 'ok' || upJson.endpoint, 'runtime-agent', 'mcpace-up-json-shape', 'up JSON did not expose ok/status/endpoint', { stdoutTail: tail(up.stdout) });

    runCheck('runtime-agent', 'mcpace-status', binaryPath, ['serve', 'status', ...common], { timeoutMs: RUNTIME_COMMAND_TIMEOUT_MS });
    runCheck('runtime-agent', 'mcpace-readiness', binaryPath, ['verify', 'readiness', ...common], { timeoutMs: RUNTIME_COMMAND_TIMEOUT_MS });
  } finally {
    runCheck('runtime-agent', 'mcpace-stop', binaryPath, ['serve', 'stop', '--root', root, '--json'], { timeoutMs: RUNTIME_COMMAND_TIMEOUT_MS });
  }

  const stopped = runCheck('runtime-agent', 'mcpace-stopped-status-custom-port', binaryPath, [
    'serve', 'status', '--root', root, '--port', String(port), '--json',
  ], { timeoutMs: RUNTIME_COMMAND_TIMEOUT_MS });
  const stoppedJson = JSON.parse(stopped.stdout);
  const text = JSON.stringify(stoppedJson);
  expect(text.includes(String(port)), 'runtime-agent', 'stopped-status-preserves-custom-port', 'stopped status did not preserve custom port', { port, stdoutTail: tail(stopped.stdout) });
}

async function main() {
  const startedAt = now();
  const agentTemp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-agent-checks-'));
  const expectedVersion = deriveProjectVersion();

  try {
    const envFacts = {
      node: runCheck('env-agent', 'node-version', 'node', ['--version']).stdout.trim(),
      npm: runCheck('env-agent', 'npm-version', 'npm', ['--version']).stdout.trim(),
      cargo: runCheck('env-agent', 'cargo-version', 'cargo', ['--version']).stdout.trim(),
      rustc: runCheck('env-agent', 'rustc-version', 'rustc', ['--version']).stdout.trim(),
      rustfmt: runCheck('env-agent', 'rustfmt-version', 'rustfmt', ['--version']).stdout.trim(),
    };
    checks.push({ agent: 'env-agent', id: 'version-summary', command: 'internal summary', cwd: repoRoot, status: 'pass', durationMs: 0, facts: envFacts });

    runCheck('metadata-agent', 'version-alignment', 'node', ['-e', `
      const fs=require('fs');
      const cargo=fs.readFileSync('Cargo.toml','utf8').match(/^version\\s*=\\s*"([^"]+)"/m)?.[1];
      const root=require('./package.json').version;
      const cli=require('./packages/npm/cli/package.json').version;
      if (!(cargo && cargo === root && root === cli)) throw new Error(JSON.stringify({cargo,root,cli}));
    `]);

    runCheck('static-agent', 'npm-check', 'npm', ['run', 'check']);
    runCheck('risk-agent', 'deep-risk-audit', 'npm', ['run', 'audit:deep']);
    runCheck('rust-agent', 'cargo-generate-lockfile', 'cargo', ['generate-lockfile']);
    runCheck('rust-agent', 'cargo-fmt', 'cargo', ['fmt', '--check']);
    runCheck('rust-agent', 'cargo-test', 'cargo', ['test', '--locked', '--', '--test-threads=1']);
    runCheck('rust-agent', 'cargo-clippy', 'cargo', ['clippy', '--all-targets', '--locked', '--', '-D', 'warnings']);
    runCheck('dependency-agent', 'cargo-tree-duplicates', 'cargo', ['tree', '--duplicates', '--locked']);
    runCheck('dependency-agent', 'cargo-tree-features', 'cargo', ['tree', '-e', 'features', '--locked']);
    runCheck('rust-agent', 'cargo-build-release', 'cargo', ['build', '--release', '--locked']);

    const binaryPath = path.join(repoRoot, 'target', 'release', process.platform === 'win32' ? 'mcpace.exe' : 'mcpace');
    expect(fs.existsSync(binaryPath), 'rust-agent', 'release-binary-exists', `missing release binary ${binaryPath}`, { binaryPath });
    const version = runCheck('rust-agent', 'release-binary-version', binaryPath, ['--version']);
    expect(version.stdout.trim() === expectedVersion, 'rust-agent', 'release-binary-version-matches', 'release binary version mismatch', {
      expectedVersion,
      actualVersion: version.stdout.trim(),
    });

    runNodeSmoke(agentTemp, binaryPath, expectedVersion);
    await runRuntimeSmoke(agentTemp, binaryPath);

    if (skipRelease) {
      checks.push({ agent: 'release-agent', id: 'source-zip', command: 'skipped by --skip-release', cwd: repoRoot, status: 'skipped', durationMs: 0 });
    } else {
      runCheck('release-agent', 'source-zip', 'npm', ['run', 'build:source-zip']);
    }

    const summary = {
      schema: 'mcpace.agentChecks.v1',
      generatedAt: now(),
      startedAt,
      repoRoot,
      status: 'pass',
      projectVersion: expectedVersion,
      checkCount: checks.length,
      agents: Object.fromEntries(
        [...new Set(checks.map((check) => check.agent))].sort().map((agent) => [
          agent,
          {
            checks: checks.filter((check) => check.agent === agent).length,
            failed: checks.filter((check) => check.agent === agent && check.status === 'failed').length,
            skipped: checks.filter((check) => check.agent === agent && check.status === 'skipped').length,
          },
        ]),
      ),
      checks,
    };
    if (!keepArtifacts) {
      fs.rmSync(agentTemp, { recursive: true, force: true });
    } else {
      summary.tempArtifactRoot = agentTemp;
    }
    process.stdout.write(jsonOutput ? `${JSON.stringify(summary, null, 2)}\n` : `agent checks passed (${summary.checkCount} checks)\n`);
  } catch (error) {
    const failed = {
      schema: 'mcpace.agentChecks.v1',
      generatedAt: now(),
      startedAt,
      repoRoot,
      status: 'failed',
      error: error?.message ?? String(error),
      checkCount: checks.length,
      checks,
      tempArtifactRoot: agentTemp,
    };
    process.stdout.write(jsonOutput ? `${JSON.stringify(failed, null, 2)}\n` : `${failed.error}\n`);
    process.exitCode = 1;
  }
}

main();
