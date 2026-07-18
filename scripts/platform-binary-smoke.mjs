#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { platformSmokeCommands } from './lib/platform-smoke-commands.mjs';
import { childEnvForCommand } from './lib/safe-child-env.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, '..');
const WINDOWS_AGENT_LAUNCHER_NAME = 'mcpace-agent-launcher.exe';

function usage() {
  return 'Usage: node scripts/platform-binary-smoke.mjs --binary <path-to-mcpace[.exe]> [--json]';
}

function parseArgs(argv) {
  const parsed = { binary: null, json: false };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--binary') {
      parsed.binary = argv[index + 1] ?? null;
      index += 1;
    } else if (arg === '--json') {
      parsed.json = true;
    } else if (arg === '-h' || arg === '--help') {
      parsed.help = true;
    } else {
      throw new Error(`unsupported argument: ${arg}\n${usage()}`);
    }
  }
  return parsed;
}

function defaultBinary() {
  const name = process.platform === 'win32' ? 'mcpace.exe' : 'mcpace';
  return path.join(repoRoot, 'target', 'release', name);
}

function commandMatrix() {
  return platformSmokeCommands().map((item) => ({
    ...item,
    mustContain:
      item.args.length === 1 && item.args[0] === 'help'
        ? /MCPace/
        : item.args.length === 1 && item.args[0] === 'version'
          ? /\d+\.\d+\.\d+/
          : item.expects === 'text'
            ? /Usage:/
            : null,
  }));
}

function parseJson(output) {
  try {
    JSON.parse(output);
    return true;
  } catch {
    return false;
  }
}

function windowsSidecarChecks(binary) {
  if (path.basename(binary).toLowerCase() !== 'mcpace.exe') return [];
  const launcher = path.join(path.dirname(binary), WINDOWS_AGENT_LAUNCHER_NAME);
  try {
    const stat = fs.lstatSync(launcher);
    const ok = stat.isFile() && !stat.isSymbolicLink();
    return [{
      id: 'windows-hidden-autostart-launcher',
      ok,
      path: launcher,
      reason: ok
        ? `${WINDOWS_AGENT_LAUNCHER_NAME} is present next to mcpace.exe`
        : `${WINDOWS_AGENT_LAUNCHER_NAME} must be a regular file and not a symlink`,
      sizeBytes: stat.size,
    }];
  } catch (error) {
    return [{
      id: 'windows-hidden-autostart-launcher',
      ok: false,
      path: launcher,
      reason: `missing required Windows autostart sidecar: ${error?.message ?? error}`,
      sizeBytes: 0,
    }];
  }
}

function run(binary, item) {
  const result = spawnSync(binary, item.args, {
    cwd: repoRoot,
    encoding: 'utf8',
    env: childEnvForCommand(binary),
    timeout: 15_000,
    windowsHide: true,
  });
  const stdout = result.stdout ?? '';
  const stderr = result.stderr ?? '';
  const failureDetail =
    result.error?.message || stderr.trim() || stdout.trim() || `signal=${result.signal}`;
  let ok = !result.error && result.status === 0;
  let reason = ok ? 'ok' : `exit=${result.status}; ${failureDetail}`;
  if (ok && item.expects === 'json' && !parseJson(stdout)) {
    ok = false;
    reason = 'stdout was not valid JSON';
  }
  if (item.expects === 'jsonOrStatusOne') {
    ok =
      !result.error &&
      (result.status === 0 || result.status === 1) &&
      parseJson(stdout);
    reason = ok
      ? result.status === 0
        ? 'ok JSON'
        : 'accepted JSON not-ready status'
      : 'expected valid JSON with exit status 0 or 1';
  }
  if (ok && item.mustContain && !item.mustContain.test(stdout)) {
    ok = false;
    reason = `stdout did not match ${item.mustContain}`;
  }
  return {
    command: item.args.join(' '),
    status: result.status,
    ok,
    reason,
    stdoutBytes: Buffer.byteLength(stdout),
    stderrBytes: Buffer.byteLength(stderr),
  };
}

let parsed;
try {
  parsed = parseArgs(process.argv.slice(2));
} catch (error) {
  process.stderr.write(`${error.message}\n`);
  process.exit(2);
}

if (parsed.help) {
  process.stdout.write(`${usage()}\n`);
  process.exit(0);
}

const binary = path.resolve(parsed.binary ?? defaultBinary());
if (!fs.existsSync(binary)) {
  process.stderr.write(`binary not found: ${binary}\nRun cargo build --release first.\n`);
  process.exit(2);
}

const results = commandMatrix().map((item) => run(binary, item));
const sidecarChecks = windowsSidecarChecks(binary);
const report = {
  schema: 'mcpace.platformBinarySmoke.v1',
  generatedAt: new Date().toISOString(),
  platform: process.platform,
  arch: process.arch,
  binary,
  summary: {
    total: results.length,
    pass: results.filter((item) => item.ok).length,
    fail: results.filter((item) => !item.ok).length,
    sidecarTotal: sidecarChecks.length,
    sidecarPass: sidecarChecks.filter((item) => item.ok).length,
    sidecarFail: sidecarChecks.filter((item) => !item.ok).length,
  },
  results,
  sidecarChecks,
};

if (parsed.json) {
  process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
} else {
  process.stdout.write(`MCPace platform binary smoke: ${report.summary.pass}/${report.summary.total} passed on ${report.platform}/${report.arch}\n`);
  for (const item of results) {
    process.stdout.write(`${item.ok ? 'PASS' : 'FAIL'} ${item.command} - ${item.reason}\n`);
  }
  for (const item of sidecarChecks) {
    process.stdout.write(`${item.ok ? 'PASS' : 'FAIL'} ${item.id} - ${item.reason}\n`);
  }
}

if (report.summary.fail > 0 || report.summary.sidecarFail > 0) {
  process.exit(1);
}
