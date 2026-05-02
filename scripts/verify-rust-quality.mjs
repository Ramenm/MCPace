#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { spawnSync } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { childEnvForCommand } from './lib/safe-child-env.mjs';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const DEFAULT_OUTPUT_PATH = path.join(repoRoot, 'reports', 'rust-quality-latest.json');
const DEFAULT_TIMEOUT_MS = 300_000;
const DEFAULT_MAX_BUFFER_BYTES = 16 * 1024 * 1024;

function parsePositiveIntegerEnv(name, fallback) {
  const parsed = Number.parseInt(process.env[name] || '', 10);
  return Number.isSafeInteger(parsed) && parsed > 0 ? parsed : fallback;
}

const DEFAULTS = {
  timeoutMs: parsePositiveIntegerEnv('MCPACE_RUST_QUALITY_TIMEOUT_MS', DEFAULT_TIMEOUT_MS),
  maxBufferBytes: parsePositiveIntegerEnv('MCPACE_RUST_QUALITY_MAX_BUFFER_BYTES', DEFAULT_MAX_BUFFER_BYTES),
};

function parseArgs(argv) {
  const options = {
    json: false,
    write: null,
    planOnly: false,
    allowMissingCargo: false,
    timeoutMs: DEFAULTS.timeoutMs,
    maxBufferBytes: DEFAULTS.maxBufferBytes,
    skipFmt: false,
    skipClippy: false,
    skipTests: false,
    skipBuild: false,
    help: false,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    switch (arg) {
      case '--json':
        options.json = true;
        break;
      case '--write': {
        const value = argv[++index];
        if (!value) {
          throw new Error('verify-rust-quality requires a path after --write');
        }
        options.write = value;
        break;
      }
      case '--plan-only':
        options.planOnly = true;
        break;
      case '--allow-missing-cargo':
        options.allowMissingCargo = true;
        break;
      case '--timeout-ms':
        options.timeoutMs = parsePositiveInteger(argv[++index], '--timeout-ms');
        break;
      case '--max-buffer-bytes':
        options.maxBufferBytes = parsePositiveInteger(argv[++index], '--max-buffer-bytes');
        break;
      case '--skip-fmt':
        options.skipFmt = true;
        break;
      case '--skip-clippy':
        options.skipClippy = true;
        break;
      case '--skip-tests':
        options.skipTests = true;
        break;
      case '--skip-build':
        options.skipBuild = true;
        break;
      case '--help':
      case '-h':
        options.help = true;
        break;
      default:
        throw new Error(`unsupported verify-rust-quality argument: ${arg}`);
    }
  }

  return options;
}

function parsePositiveInteger(value, label) {
  if (!/^\d+$/.test(String(value || ''))) {
    throw new Error(`${label} must be a positive integer`);
  }
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) {
    throw new Error(`${label} must be a positive integer`);
  }
  return parsed;
}

function printHelp() {
  console.log(`Usage: node scripts/verify-rust-quality.mjs [--json] [--write <path>] [--plan-only] [--allow-missing-cargo]

Runs the Rust host quality gate in the same order CI should prove it:
  1. cargo fmt --all -- --check
  2. cargo clippy --all-targets --locked -- -D warnings
  3. node scripts/run-rust-tests.mjs --json --profile non-lifecycle
  4. cargo build --release --locked

Use --plan-only for contract tests and --allow-missing-cargo only in constrained
environments where the host proof should be reported as partial rather than
silently implied.`);
}

function commandExists(command) {
  const probe = process.platform === 'win32'
    ? spawnSync('where.exe', [command], { encoding: 'utf8', windowsHide: true })
    : spawnSync('sh', ['-c', `command -v ${shellQuote(command)} >/dev/null 2>&1`], { encoding: 'utf8' });
  return probe.status === 0;
}

function shellQuote(value) {
  return `'${String(value).replaceAll("'", `'\\''`)}'`;
}

function resolveCommandInvocation(command, args = []) {
  if (process.platform === 'win32' && command === 'cargo') {
    return {
      bin: 'cmd.exe',
      args: ['/d', '/s', '/c', 'cargo', ...args],
      displayCommand: ['cargo', ...args].join(' '),
    };
  }

  return {
    bin: command,
    args,
    displayCommand: [command, ...args].join(' '),
  };
}

function rustQualityPlan(options = {}) {
  const lanes = [];
  if (!options.skipFmt) {
    lanes.push({ name: 'fmt', command: ['cargo', ['fmt', '--all', '--', '--check']] });
  }
  if (!options.skipClippy) {
    lanes.push({ name: 'clippy', command: ['cargo', ['clippy', '--all-targets', '--locked', '--', '-D', 'warnings']] });
  }
  if (!options.skipTests) {
    lanes.push({ name: 'rust-tests', command: [process.execPath, ['scripts/run-rust-tests.mjs', '--json', '--profile', 'non-lifecycle']] });
  }
  if (!options.skipBuild) {
    lanes.push({ name: 'release-build', command: ['cargo', ['build', '--release', '--locked']] });
  }
  return lanes;
}

function firstNonEmptyLine(value) {
  return String(value || '')
    .split(/\r?\n/)
    .map((line) => line.trim())
    .find(Boolean) || null;
}

function detectVersion(command, args = ['--version']) {
  const invocation = resolveCommandInvocation(command, args);
  const result = spawnSync(invocation.bin, invocation.args, {
    cwd: repoRoot,
    encoding: 'utf8',
    timeout: 5_000,
    maxBuffer: 256 * 1024,
    windowsHide: true,
  });
  if (result.status !== 0 || result.error) {
    return null;
  }
  return firstNonEmptyLine(result.stdout) || firstNonEmptyLine(result.stderr);
}

function summarizeOutput(value) {
  return String(value || '')
    .split(/\r?\n/)
    .filter(Boolean)
    .slice(-20)
    .join('\n');
}

function runLane(lane, options) {
  const [command, args] = lane.command;
  const invocation = resolveCommandInvocation(command, args);
  const startedAt = Date.now();
  const env = childEnvForCommand(command);

  const result = spawnSync(invocation.bin, invocation.args, {
    cwd: repoRoot,
    encoding: 'utf8',
    env,
    timeout: options.timeoutMs,
    maxBuffer: options.maxBufferBytes,
    windowsHide: true,
  });

  const timedOut = result.error?.code === 'ETIMEDOUT';
  const ok = result.status === 0 && !timedOut && !result.error;
  return {
    name: lane.name,
    command: invocation.displayCommand,
    status: ok ? 'pass' : timedOut ? 'timeout' : 'fail',
    ok,
    code: result.status,
    signal: result.signal ?? null,
    durationMs: Date.now() - startedAt,
    timeoutMs: options.timeoutMs,
    maxBufferBytes: options.maxBufferBytes,
    stdoutSummary: summarizeOutput(result.stdout),
    stderrSummary: summarizeOutput(result.stderr),
    error: result.error ? String(result.error.message || result.error) : null,
  };
}

function plannedLane(lane) {
  const [command, args] = lane.command;
  const invocation = resolveCommandInvocation(command, args);
  return {
    name: lane.name,
    command: invocation.displayCommand,
    status: 'planned',
    ok: true,
  };
}

function skippedLane(lane, reason) {
  const [command, args] = lane.command;
  const invocation = resolveCommandInvocation(command, args);
  return {
    name: lane.name,
    command: invocation.displayCommand,
    status: 'skipped',
    ok: true,
    reason,
  };
}

function statusFromLanes(lanes, planOnly = false) {
  if (planOnly) {
    return 'planned';
  }
  if (lanes.some((lane) => lane.status === 'fail' || lane.status === 'timeout')) {
    return 'fail';
  }
  if (lanes.some((lane) => lane.status === 'skipped')) {
    return 'partial';
  }
  return 'pass';
}

function verifyRustQuality(options = {}) {
  const normalized = { ...parseArgs([]), ...options };
  const plan = rustQualityPlan(normalized);
  const startedAt = Date.now();
  const cargoAvailable = commandExists('cargo');
  const cargoVersion = cargoAvailable ? detectVersion('cargo') : null;
  const rustcVersion = commandExists('rustc') ? detectVersion('rustc') : null;

  let lanes;
  if (normalized.planOnly) {
    lanes = plan.map(plannedLane);
  } else if (!cargoAvailable && normalized.allowMissingCargo) {
    lanes = plan.map((lane) => skippedLane(lane, 'cargo is not available on this host'));
  } else {
    lanes = [];
    for (const lane of plan) {
      const result = runLane(lane, normalized);
      lanes.push(result);
      if (!result.ok) {
        break;
      }
    }
  }

  return {
    ok: lanes.every((lane) => lane.ok),
    status: statusFromLanes(lanes, normalized.planOnly),
    generatedAt: new Date().toISOString(),
    durationMs: Date.now() - startedAt,
    toolchain: {
      cargoAvailable,
      cargoVersion,
      rustcVersion,
      rustupToolchain: process.env.RUSTUP_TOOLCHAIN || null,
    },
    policy: {
      laneOrder: ['fmt', 'clippy', 'rust-tests', 'release-build'],
      clippyArgs: ['--all-targets', '--locked', '--', '-D', 'warnings'],
      releaseBuildArgs: ['--release', '--locked'],
      testsProfile: 'non-lifecycle',
    },
    lanes,
  };
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.help) {
    printHelp();
    return;
  }

  const report = verifyRustQuality(options);
  const outputPath = options.write ? path.resolve(repoRoot, options.write) : null;
  if (outputPath) {
    fs.mkdirSync(path.dirname(outputPath), { recursive: true });
    fs.writeFileSync(outputPath, `${JSON.stringify(report, null, 2)}\n`);
  }

  if (options.json) {
    process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
  } else {
    console.log(`rust quality: ${report.status}`);
    for (const lane of report.lanes) {
      console.log(`- ${lane.name}: ${lane.status} (${lane.command})`);
    }
  }

  if (report.status === 'fail') {
    process.exitCode = 1;
  }
  if (report.status === 'partial' && !options.allowMissingCargo) {
    process.exitCode = 1;
  }
}

function isCliInvocation() {
  const entry = process.argv[1];
  return entry ? pathToFileURL(path.resolve(entry)).href === import.meta.url : false;
}

if (isCliInvocation()) {
  main().catch((error) => {
    console.error(error.message || String(error));
    process.exitCode = 1;
  });
}

export { parseArgs, rustQualityPlan, verifyRustQuality };
