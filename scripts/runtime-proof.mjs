#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { repoRoot } from './lib/project-metadata.mjs';

const DEFAULT_DURATION_MS = 30_000;
const DEFAULT_CONCURRENCY = 64;

function printHelp() {
  console.log(`Usage: node scripts/runtime-proof.mjs [--json] [--out-dir <dir>] [--duration-ms <n>] [--concurrency <n>] [--allow-missing-rust] [--plan-only]\n\nBuilds and proves the real native stack on this host: Rust checks, release/perf builds, native smoke, load tests, latency summaries, and release-vs-perf comparison.\n\nThis script is intentionally host-local. Cross-platform release readiness still requires running it on each supported OS/architecture lane.`);
}

function parseArgs(argv) {
  const getValue = (name, fallback = '') => {
    const index = argv.indexOf(name);
    return index >= 0 ? argv[index + 1] || fallback : fallback;
  };
  return {
    help: argv.includes('-h') || argv.includes('--help'),
    json: argv.includes('--json'),
    planOnly: argv.includes('--plan-only'),
    allowMissingRust: argv.includes('--allow-missing-rust'),
    outDir: path.resolve(getValue('--out-dir', path.join(repoRoot, 'reports', 'runtime-proof'))),
    durationMs: positiveInteger(getValue('--duration-ms', String(DEFAULT_DURATION_MS)), '--duration-ms'),
    concurrency: positiveInteger(getValue('--concurrency', String(DEFAULT_CONCURRENCY)), '--concurrency'),
  };
}

function positiveInteger(value, label) {
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) throw new Error(`${label} must be a positive integer`);
  return parsed;
}

function commandExists(command) {
  const probe = process.platform === 'win32'
    ? spawnSync('where.exe', [command], { encoding: 'utf8', windowsHide: true })
    : spawnSync('command', ['-v', command], { encoding: 'utf8', shell: true });
  return probe.status === 0;
}

function npmCommand(args) {
  const npmExecPath = process.env.npm_execpath;
  if (npmExecPath && fs.existsSync(npmExecPath)) return [process.execPath, npmExecPath, ...args];
  if (process.platform === 'win32') return ['cmd.exe', '/d', '/s', '/c', 'npm.cmd', ...args];
  return ['npm', ...args];
}

function binaryName() {
  return process.platform === 'win32' ? 'mcpace.exe' : 'mcpace';
}

function releaseBinary() {
  return path.join(repoRoot, 'target', 'release', binaryName());
}

function perfBinary() {
  return path.join(repoRoot, 'target', 'perf', binaryName());
}

function runStep(id, command, args, options = {}) {
  const started = Date.now();
  const result = spawnSync(command, args, {
    cwd: options.cwd || repoRoot,
    encoding: 'utf8',
    timeout: options.timeoutMs || 20 * 60_000,
    windowsHide: true,
    shell: false,
  });
  const stdout = result.stdout || '';
  const stderr = result.stderr || '';
  return {
    id,
    command: [command, ...args].join(' '),
    status: result.error ? 'error' : result.status === 0 ? 'pass' : 'fail',
    exitCode: result.status,
    signal: result.signal,
    durationMs: Date.now() - started,
    stdoutTail: stdout.slice(-8000),
    stderrTail: stderr.slice(-8000),
    error: result.error ? String(result.error.message || result.error) : null,
  };
}

function runNpmStep(id, args, options = {}) {
  const [command, ...commandArgs] = npmCommand(args);
  return runStep(id, command, commandArgs, options);
}

function writeText(file, text) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, text);
}

function readToolchain() {
  const file = path.join(repoRoot, 'rust-toolchain.toml');
  if (!fs.existsSync(file)) return null;
  return fs.readFileSync(file, 'utf8').match(/^\s*channel\s*=\s*"([^"]+)"/m)?.[1] || null;
}

function hostInfo() {
  const node = runStep('node-version', process.execPath, ['--version'], { timeoutMs: 30_000 });
  const npm = runNpmStep('npm-version', ['--version'], { timeoutMs: 30_000 });
  const cargo = commandExists(process.platform === 'win32' ? 'cargo.exe' : 'cargo')
    ? runStep('cargo-version', process.platform === 'win32' ? 'cargo.exe' : 'cargo', ['--version'], { timeoutMs: 30_000 })
    : null;
  const rustc = commandExists(process.platform === 'win32' ? 'rustc.exe' : 'rustc')
    ? runStep('rustc-version', process.platform === 'win32' ? 'rustc.exe' : 'rustc', ['--version'], { timeoutMs: 30_000 })
    : null;
  return {
    platform: process.platform,
    arch: process.arch,
    osType: os.type(),
    osRelease: os.release(),
    node: node.stdoutTail.trim(),
    npm: npm.stdoutTail.trim(),
    cargo: cargo?.stdoutTail.trim() || null,
    rustc: rustc?.stdoutTail.trim() || null,
    pinnedRustToolchain: readToolchain(),
  };
}

function plannedSteps(args) {
  return [
    { id: 'rust-contracts', command: npmCommand(['run', 'check:rust']).join(' ') },
    { id: 'build-release', command: npmCommand(['run', 'build']).join(' ') },
    { id: 'build-perf', command: npmCommand(['run', 'build:perf']).join(' ') },
    { id: 'smoke-release', command: npmCommand(['run', 'platform:binary-smoke', '--', '--binary', releaseBinary()]).join(' ') },
    { id: 'load-release', command: npmCommand(['run', 'load:local', '--', '--binary', releaseBinary(), '--duration-ms', String(args.durationMs), '--concurrency', String(args.concurrency), '--json']).join(' ') },
    { id: 'check-load-release', command: npmCommand(['run', 'check:load-result', '--', '<release-report.json>']).join(' ') },
    { id: 'latency-release', command: npmCommand(['run', 'latency:report', '--', '<release-report.json>', '--json']).join(' ') },
    { id: 'smoke-perf', command: npmCommand(['run', 'platform:binary-smoke', '--', '--binary', perfBinary()]).join(' ') },
    { id: 'load-perf', command: npmCommand(['run', 'load:local', '--', '--binary', perfBinary(), '--duration-ms', String(args.durationMs), '--concurrency', String(args.concurrency), '--json']).join(' ') },
    { id: 'check-load-perf', command: npmCommand(['run', 'check:load-result', '--', '<perf-report.json>']).join(' ') },
    { id: 'latency-perf', command: npmCommand(['run', 'latency:report', '--', '<perf-report.json>', '--json']).join(' ') },
    { id: 'compare-release-perf', command: npmCommand(['run', 'latency:compare', '--', '<release-report.json>', '<perf-report.json>', '--json']).join(' ') },
  ];
}

function runRuntimeProof(args) {
  const cargoAvailable = commandExists(process.platform === 'win32' ? 'cargo.exe' : 'cargo');
  const steps = [];
  if (args.planOnly) {
    return {
      schema: 'mcpace.runtimeProof.v1',
      generatedAt: new Date().toISOString(),
      mode: { planOnly: true, durationMs: args.durationMs, concurrency: args.concurrency },
      host: hostInfo(),
      overall: 'planned',
      steps: plannedSteps(args).map((step) => ({ ...step, status: 'planned' })),
    };
  }

  if (!cargoAvailable) {
    return {
      schema: 'mcpace.runtimeProof.v1',
      generatedAt: new Date().toISOString(),
      mode: { planOnly: false, durationMs: args.durationMs, concurrency: args.concurrency, allowMissingRust: args.allowMissingRust },
      host: hostInfo(),
      overall: 'blocked',
      reason: `Cargo is not on PATH. Install Rust ${readToolchain() || 'toolchain'} and rerun npm run proof:runtime.`,
      steps,
    };
  }

  fs.mkdirSync(args.outDir, { recursive: true });

  for (const step of [
    () => runNpmStep('rust-contracts', ['run', 'check:rust']),
    () => runNpmStep('build-release', ['run', 'build']),
    () => runNpmStep('build-perf', ['run', 'build:perf']),
    () => runNpmStep('smoke-release', ['run', 'platform:binary-smoke', '--', '--binary', releaseBinary()]),
  ]) {
    const result = step();
    steps.push(result);
    if (result.status !== 'pass') return finalReport(args, steps, 'fail');
  }

  const releaseReport = path.join(args.outDir, 'load-release.json');
  const perfReport = path.join(args.outDir, 'load-perf.json');
  const releaseLatency = path.join(args.outDir, 'latency-release.json');
  const perfLatency = path.join(args.outDir, 'latency-perf.json');
  const comparison = path.join(args.outDir, 'latency-release-vs-perf.json');

  const loadRelease = runNpmStep('load-release', ['run', 'load:local', '--', '--binary', releaseBinary(), '--duration-ms', String(args.durationMs), '--concurrency', String(args.concurrency), '--json'], { timeoutMs: args.durationMs * 8 + 120_000 });
  steps.push(loadRelease);
  writeText(releaseReport, loadRelease.stdoutTail);
  if (loadRelease.status !== 'pass') return finalReport(args, steps, 'fail');

  for (const step of [
    () => runNpmStep('check-load-release', ['run', 'check:load-result', '--', releaseReport]),
    () => runNpmStep('latency-release', ['run', 'latency:report', '--', releaseReport, '--json']),
    () => runNpmStep('smoke-perf', ['run', 'platform:binary-smoke', '--', '--binary', perfBinary()]),
  ]) {
    const result = step();
    steps.push(result);
    if (result.id === 'latency-release') writeText(releaseLatency, result.stdoutTail);
    if (result.status !== 'pass') return finalReport(args, steps, 'fail');
  }

  const loadPerf = runNpmStep('load-perf', ['run', 'load:local', '--', '--binary', perfBinary(), '--duration-ms', String(args.durationMs), '--concurrency', String(args.concurrency), '--json'], { timeoutMs: args.durationMs * 8 + 120_000 });
  steps.push(loadPerf);
  writeText(perfReport, loadPerf.stdoutTail);
  if (loadPerf.status !== 'pass') return finalReport(args, steps, 'fail');

  for (const step of [
    () => runNpmStep('check-load-perf', ['run', 'check:load-result', '--', perfReport]),
    () => runNpmStep('latency-perf', ['run', 'latency:report', '--', perfReport, '--json']),
    () => runNpmStep('compare-release-perf', ['run', 'latency:compare', '--', releaseReport, perfReport, '--json']),
  ]) {
    const result = step();
    steps.push(result);
    if (result.id === 'latency-perf') writeText(perfLatency, result.stdoutTail);
    if (result.id === 'compare-release-perf') writeText(comparison, result.stdoutTail);
    if (result.status !== 'pass') return finalReport(args, steps, 'fail');
  }

  return finalReport(args, steps, 'pass');
}

function finalReport(args, steps, overall) {
  return {
    schema: 'mcpace.runtimeProof.v1',
    generatedAt: new Date().toISOString(),
    mode: { planOnly: false, durationMs: args.durationMs, concurrency: args.concurrency, allowMissingRust: args.allowMissingRust },
    host: hostInfo(),
    outDir: args.outDir,
    overall,
    summary: {
      pass: steps.filter((step) => step.status === 'pass').length,
      fail: steps.filter((step) => step.status === 'fail' || step.status === 'error').length,
      total: steps.length,
    },
    artifacts: {
      releaseLoad: path.join(args.outDir, 'load-release.json'),
      perfLoad: path.join(args.outDir, 'load-perf.json'),
      releaseLatency: path.join(args.outDir, 'latency-release.json'),
      perfLatency: path.join(args.outDir, 'latency-perf.json'),
      comparison: path.join(args.outDir, 'latency-release-vs-perf.json'),
    },
    steps,
  };
}

function renderText(report) {
  if (report.overall === 'blocked') return `MCPace runtime proof: blocked - ${report.reason}\n`;
  if (report.overall === 'planned') return `MCPace runtime proof plan: ${report.steps.length} steps\n`;
  return `MCPace runtime proof: ${report.overall} (${report.summary.pass} pass, ${report.summary.fail} fail)\nArtifacts: ${report.outDir}\n`;
}

try {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    printHelp();
  } else {
    const report = runRuntimeProof(args);
    if (!args.planOnly) {
      fs.mkdirSync(args.outDir, { recursive: true });
      fs.writeFileSync(path.join(args.outDir, 'runtime-proof.json'), `${JSON.stringify(report, null, 2)}\n`);
    }
    if (args.json) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
    else process.stdout.write(renderText(report));
    if (report.overall === 'fail' || (report.overall === 'blocked' && !args.allowMissingRust)) process.exitCode = 1;
  }
} catch (error) {
  process.stderr.write(`${error?.message || String(error)}\n`);
  process.exitCode = 2;
}
