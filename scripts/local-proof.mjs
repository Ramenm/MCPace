#!/usr/bin/env node
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, '..');
const rawArgs = process.argv.slice(2);
const args = new Set(rawArgs);
const jsonOnly = args.has('--json');
const write = args.has('--write');
const full = args.has('--full');
const planOnly = args.has('--plan-only');
const install = args.has('--install');
const outputArgIndex = rawArgs.indexOf('--out-dir');
const outDir = path.resolve(outputArgIndex >= 0 ? rawArgs[outputArgIndex + 1] ?? path.join(repoRoot, 'reports') : path.join(repoRoot, 'reports'));

function help() {
  return [
    'Usage: node scripts/local-proof.mjs [--write] [--json] [--full] [--install] [--plan-only] [--out-dir <dir>]',
    '',
    'Runs a host-local proof pass for the current OS.',
    '--full       Also run Rust checks/build/native binary smoke when Cargo is available.',
    '--install    Run npm ci before checks.',
    '--write      Write reports/local-proof-<platform>.json and .md.',
    '--plan-only  Print the command plan without executing it.',
  ].join('\n');
}

if (args.has('-h') || args.has('--help')) {
  process.stdout.write(`${help()}\n`);
  process.exit(0);
}

function commandExists(command) {
  const probe = process.platform === 'win32'
    ? spawnSync('where.exe', [command], { encoding: 'utf8', windowsHide: true })
    : spawnSync('command', ['-v', command], { encoding: 'utf8', shell: true });
  return probe.status === 0;
}

function runCapture(command, commandArgs, options = {}) {
  const started = Date.now();
  const result = spawnSync(command, commandArgs, {
    cwd: repoRoot,
    encoding: 'utf8',
    timeout: options.timeoutMs ?? 300_000,
    windowsHide: true,
    shell: false,
  });
  return {
    command: [command, ...commandArgs].join(' '),
    status: result.error ? 'failed' : result.status === 0 ? 'pass' : 'fail',
    exitCode: result.status,
    signal: result.signal,
    durationMs: Date.now() - started,
    stdoutTail: (result.stdout ?? '').slice(-4000),
    stderrTail: (result.stderr ?? '').slice(-4000),
    error: result.error ? String(result.error.message ?? result.error) : null,
  };
}

function runInherited(command, commandArgs, options = {}) {
  const started = Date.now();
  const result = spawnSync(command, commandArgs, {
    cwd: repoRoot,
    stdio: 'inherit',
    timeout: options.timeoutMs ?? 300_000,
    windowsHide: true,
    shell: false,
  });
  return {
    command: [command, ...commandArgs].join(' '),
    status: result.error ? 'failed' : result.status === 0 ? 'pass' : 'fail',
    exitCode: result.status,
    signal: result.signal,
    durationMs: Date.now() - started,
    stdoutTail: '',
    stderrTail: '',
    error: result.error ? String(result.error.message ?? result.error) : null,
  };
}

function npmCommandParts(commandArgs = []) {
  const npmExecPath = process.env.npm_execpath;
  if (npmExecPath && fs.existsSync(npmExecPath)) {
    return [process.execPath, npmExecPath, ...commandArgs];
  }

  if (process.platform === 'win32') {
    return ['cmd.exe', '/d', '/s', '/c', 'npm.cmd', ...commandArgs];
  }

  return ['npm', ...commandArgs];
}

function cargoCommand() {
  return process.platform === 'win32' ? 'cargo.exe' : 'cargo';
}

function targetBinary() {
  return path.join(repoRoot, 'target', 'release', process.platform === 'win32' ? 'mcpace.exe' : 'mcpace');
}

function readToolchain() {
  const file = path.join(repoRoot, 'rust-toolchain.toml');
  if (!fs.existsSync(file)) return null;
  const text = fs.readFileSync(file, 'utf8');
  return text.match(/^\s*channel\s*=\s*"([^"]+)"\s*$/m)?.[1] ?? null;
}

function commandPlan() {
  const cargoAvailable = commandExists(cargoCommand());
  const plan = [];
  if (install) plan.push({ id: 'npm-ci', kind: 'node', required: true, command: npmCommandParts(['ci']) });
  plan.push({ id: 'node-contracts', kind: 'node', required: true, command: npmCommandParts(['run', 'check']) });
  plan.push({ id: 'npm-package-contract', kind: 'node', required: true, command: npmCommandParts(['run', 'check:package']) });
  plan.push({ id: 'release-dry-run', kind: 'release', required: true, command: npmCommandParts(['run', 'release:dry-run']) });
  plan.push({ id: 'npm-pack-dry-run', kind: 'release', required: true, command: npmCommandParts(['run', 'pack:npm:dry-run']) });
  plan.push({ id: 'source-zip-build', kind: 'release', required: true, command: npmCommandParts(['run', 'build:release-artifacts']) });

  if (full) {
    if (cargoAvailable) {
      plan.push({ id: 'rust-contracts', kind: 'rust', required: true, command: npmCommandParts(['run', 'check:rust']) });
      plan.push({ id: 'rust-release-build', kind: 'rust', required: true, command: npmCommandParts(['run', 'build']) });
      plan.push({ id: 'native-binary-smoke', kind: 'rust', required: true, command: npmCommandParts(['run', 'platform:binary-smoke', '--', '--binary', targetBinary()]) });
    } else {
      plan.push({
        id: 'rust-contracts',
        kind: 'rust',
        required: true,
        skipped: true,
        status: 'warn',
        reason: `Cargo is not on PATH. Install Rust ${readToolchain() ?? 'toolchain'} with rustup, then rerun npm run proof:local -- --full.`,
      });
    }
  } else {
    plan.push({
      id: 'rust-contracts',
      kind: 'rust',
      required: true,
      skipped: true,
      status: 'warn',
      reason: 'Rust checks were not requested. Rerun with --full for cargo fmt/clippy/test/build/native smoke.',
    });
  }
  return plan;
}

function hostInfo() {
  const node = runCapture(process.execPath, ['--version'], { timeoutMs: 30_000 });
  const [npmCommand, ...npmArgs] = npmCommandParts(['--version']);
  const npm = runCapture(npmCommand, npmArgs, { timeoutMs: 30_000 });
  const cargo = commandExists(cargoCommand()) ? runCapture(cargoCommand(), ['--version'], { timeoutMs: 30_000 }) : null;
  const rustc = commandExists(process.platform === 'win32' ? 'rustc.exe' : 'rustc') ? runCapture(process.platform === 'win32' ? 'rustc.exe' : 'rustc', ['--version'], { timeoutMs: 30_000 }) : null;
  return {
    platform: process.platform,
    arch: process.arch,
    osType: os.type(),
    osRelease: os.release(),
    hostname: os.hostname(),
    node: node.stdoutTail.trim(),
    npm: npm.stdoutTail.trim(),
    cargo: cargo?.stdoutTail.trim() ?? null,
    rustc: rustc?.stdoutTail.trim() ?? null,
    pinnedRustToolchain: readToolchain(),
  };
}

function buildReport() {
  const plan = commandPlan();
  const results = [];
  if (!planOnly) {
    for (const item of plan) {
      if (item.skipped) {
        results.push({ ...item, command: null, status: item.status ?? 'warn', durationMs: 0 });
        continue;
      }
      const [command, ...commandArgs] = item.command;
      const result = jsonOnly ? runCapture(command, commandArgs) : runInherited(command, commandArgs);
      results.push({ ...item, command: item.command.join(' '), ...result });
      if (item.required && result.status !== 'pass') break;
    }
  }

  const effective = planOnly ? plan.map((item) => item.skipped ? { ...item, command: null, status: item.status ?? 'warn' } : { ...item, command: item.command.join(' '), status: 'planned' }) : results;
  const fail = effective.filter((item) => item.status === 'fail' || item.status === 'failed').length;
  const warn = effective.filter((item) => item.status === 'warn').length;
  const pass = effective.filter((item) => item.status === 'pass').length;
  return {
    schema: 'mcpace.localProof.v1',
    generatedAt: new Date().toISOString(),
    mode: { full, install, planOnly },
    host: hostInfo(),
    rootName: path.basename(repoRoot),
    overall: fail > 0 ? 'fail' : warn > 0 ? 'warn' : 'pass',
    summary: { pass, warn, fail, total: effective.length },
    results: effective,
    nextSteps: fail > 0
      ? ['Fix the first failing command, then rerun npm run proof:local -- --full.']
      : warn > 0
        ? ['Install the pinned Rust toolchain and rerun npm run proof:local -- --full.', 'For macOS without local hardware, run the platform-proof GitHub Actions workflow.']
        : ['This host passed Node, package, release, Rust, build, and native binary smoke gates. Run the same proof on the remaining OS families.'],
  };
}

function renderMarkdown(report) {
  const lines = [];
  lines.push(`# MCPace local proof (${report.host.platform}/${report.host.arch})`);
  lines.push('');
  lines.push('Generated by `npm run proof:local`. This is the real proof for the current machine, not a cross-platform promise.');
  lines.push('');
  lines.push(`- Overall: **${report.overall}**`);
  lines.push(`- Node: ${report.host.node || 'not detected'}`);
  lines.push(`- npm: ${report.host.npm || 'not detected'}`);
  lines.push(`- Cargo: ${report.host.cargo || 'not detected'}`);
  lines.push(`- Rustc: ${report.host.rustc || 'not detected'}`);
  lines.push(`- Pinned Rust: ${report.host.pinnedRustToolchain || 'not pinned'}`);
  lines.push(`- Summary: ${report.summary.pass} pass, ${report.summary.warn} warn, ${report.summary.fail} fail`);
  lines.push('');
  lines.push('## Results');
  lines.push('');
  for (const result of report.results) {
    lines.push(`- **${result.status}** ${result.id}${result.command ? ` — \`${result.command}\`` : ''}`);
    if (result.reason) lines.push(`  - ${result.reason}`);
    if (result.error) lines.push(`  - error: ${result.error}`);
  }
  lines.push('');
  lines.push('## Next steps');
  lines.push('');
  for (const step of report.nextSteps) lines.push(`- ${step}`);
  lines.push('');
  return `${lines.join('\n')}\n`;
}

const report = buildReport();
if (write) {
  fs.mkdirSync(outDir, { recursive: true });
  const base = `local-proof-${process.platform}`;
  fs.writeFileSync(path.join(outDir, `${base}.json`), `${JSON.stringify(report, null, 2)}\n`);
  fs.writeFileSync(path.join(outDir, `${base}.md`), renderMarkdown(report));
}

if (jsonOnly) {
  process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
} else {
  process.stdout.write(`MCPace local proof: ${report.overall} (${report.summary.pass} pass, ${report.summary.warn} warn, ${report.summary.fail} fail) on ${report.host.platform}/${report.host.arch}\n`);
  if (write) process.stdout.write(`Wrote reports/local-proof-${process.platform}.md and .json\n`);
}

if (report.summary.fail > 0) process.exit(1);
