#!/usr/bin/env node
import { spawn } from 'node:child_process';
import fs from 'node:fs/promises';
import path from 'node:path';
import { repoRoot } from './lib/project-metadata.mjs';
import { defaultBinary, explicitBinaryFromEnv, positiveInteger } from './lib/runtime-probe.mjs';

function printHelp() {
  console.log(`Usage: node scripts/runtime-evidence.mjs [options]\n\nRuns the MCPace runtime evidence suite against a built native binary and writes JSON reports.\n\nOptions:\n  --binary <path>              MCPace binary. Env fallback: MCPACE_BINARY, MCPACE_BINARY_PATH, MCPACE_DEV_BINARY\n  --out-dir <path>             Report directory. Default: reports/runtime-evidence/<timestamp>\n  --duration-ms <n>            Load-test duration per scenario. Default: 30000\n  --concurrency <n>            Load-test concurrency. Default: 64\n  --sessions <n>               Session churn count. Default: 300\n  --slow-connections <n>       Slow-client connections per mode. Default: 16\n  --skip-load                  Skip load:local/check/latency report\n  --skip-session-churn         Skip session churn probe\n  --skip-slowloris             Skip slow-client probe\n  --plan                       Print commands without running them\n  --json                       Emit JSON summary`);
}

function parseArgs(argv) {
  const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
  const options = {
    binary: explicitBinaryFromEnv() || defaultBinary(),
    outDir: path.join(repoRoot, 'reports', 'runtime-evidence', timestamp),
    durationMs: 30_000,
    concurrency: 64,
    sessions: 300,
    slowConnections: 16,
    skipLoad: false,
    skipSessionChurn: false,
    skipSlowloris: false,
    plan: false,
    json: false,
    help: false,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    const readValue = () => {
      const value = argv[index + 1];
      if (!value) throw new Error(`${arg} requires a value`);
      index += 1;
      return value;
    };
    switch (arg) {
      case '--binary': options.binary = readValue(); break;
      case '--out-dir': options.outDir = path.resolve(readValue()); break;
      case '--duration-ms': options.durationMs = positiveInteger(readValue(), arg); break;
      case '--concurrency': options.concurrency = positiveInteger(readValue(), arg); break;
      case '--sessions': options.sessions = positiveInteger(readValue(), arg); break;
      case '--slow-connections': options.slowConnections = positiveInteger(readValue(), arg); break;
      case '--skip-load': options.skipLoad = true; break;
      case '--skip-session-churn': options.skipSessionChurn = true; break;
      case '--skip-slowloris': options.skipSlowloris = true; break;
      case '--plan': options.plan = true; break;
      case '--json': options.json = true; break;
      case '-h':
      case '--help': options.help = true; break;
      default: throw new Error(`unknown argument: ${arg}`);
    }
  }
  return options;
}

function commandSpec(name, script, args, output = '') {
  return { name, command: process.execPath, args: [path.join(repoRoot, script), ...args], output };
}

function buildPlan(options) {
  const commands = [];
  const loadReport = path.join(options.outDir, 'load-local.json');
  const loadCheck = path.join(options.outDir, 'load-check.json');
  const latencyReport = path.join(options.outDir, 'latency-report.json');
  const sessionChurn = path.join(options.outDir, 'session-churn.json');
  const slowloris = path.join(options.outDir, 'slowloris.json');

  if (!options.skipLoad) {
    commands.push(commandSpec('load:local', 'scripts/load-test-local.mjs', ['--binary', options.binary, '--duration-ms', String(options.durationMs), '--concurrency', String(options.concurrency), '--json'], loadReport));
    commands.push(commandSpec('check:load-result', 'scripts/check-load-result.mjs', [loadReport, '--json'], loadCheck));
    commands.push(commandSpec('latency:report', 'scripts/latency-report.mjs', [loadReport, '--json'], latencyReport));
  }
  if (!options.skipSessionChurn) {
    commands.push(commandSpec('probe:session-churn', 'scripts/session-churn-probe.mjs', ['--binary', options.binary, '--sessions', String(options.sessions), '--json'], sessionChurn));
  }
  if (!options.skipSlowloris) {
    commands.push(commandSpec('probe:slowloris', 'scripts/slowloris-probe.mjs', ['--binary', options.binary, '--connections', String(options.slowConnections), '--json'], slowloris));
  }
  return { outDir: options.outDir, commands };
}

async function runCommand(spec) {
  const started = Date.now();
  const child = spawn(spec.command, spec.args, { cwd: repoRoot, stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true });
  let stdout = '';
  let stderr = '';
  child.stdout.on('data', (chunk) => { stdout += chunk.toString('utf8'); });
  child.stderr.on('data', (chunk) => { stderr += chunk.toString('utf8'); });
  const { code, signal } = await new Promise((resolve) => child.once('exit', (code, signal) => resolve({ code, signal })));
  if (spec.output) await fs.writeFile(spec.output, stdout || '{}\n', 'utf8');
  if (stderr) await fs.writeFile(`${spec.output || path.join(repoRoot, 'runtime-evidence')}.${spec.name}.stderr.log`, stderr, 'utf8').catch(() => {});
  return {
    name: spec.name,
    output: spec.output || null,
    status: code === 0 ? 'pass' : 'failed',
    code,
    signal,
    durationMs: Date.now() - started,
    stderr: stderr.slice(0, 8192),
  };
}

function shellQuote(value) {
  const text = String(value);
  if (/^[A-Za-z0-9_./:=+-]+$/.test(text)) return text;
  return `'${text.replaceAll("'", "'\\''")}'`;
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.help) {
    printHelp();
    return;
  }
  const plan = buildPlan(options);
  const summary = {
    schema: 'mcpace.runtimeEvidence.v1',
    generatedAt: new Date().toISOString(),
    binary: options.binary,
    outDir: plan.outDir,
    plan: plan.commands.map((spec) => ({ name: spec.name, commandLine: [spec.command, ...spec.args].map(shellQuote).join(' '), output: spec.output })),
    status: 'planned',
    results: [],
  };
  if (options.plan) {
    if (options.json) process.stdout.write(`${JSON.stringify(summary, null, 2)}\n`);
    else {
      console.log(`MCPace runtime evidence plan -> ${summary.outDir}`);
      for (const command of summary.plan) console.log(`- ${command.name}: ${command.commandLine}${command.output ? ` > ${command.output}` : ''}`);
    }
    return;
  }
  await fs.mkdir(plan.outDir, { recursive: true });
  for (const spec of plan.commands) {
    const result = await runCommand(spec);
    summary.results.push(result);
    if (result.status !== 'pass') break;
  }
  summary.status = summary.results.every((result) => result.status === 'pass') && summary.results.length === plan.commands.length ? 'pass' : 'failed';
  await fs.writeFile(path.join(plan.outDir, 'runtime-evidence-summary.json'), `${JSON.stringify(summary, null, 2)}\n`, 'utf8');
  if (options.json) process.stdout.write(`${JSON.stringify(summary, null, 2)}\n`);
  else {
    console.log(`MCPace runtime evidence: ${summary.status}`);
    console.log(`outDir: ${summary.outDir}`);
    for (const result of summary.results) console.log(`- ${result.status} ${result.name} (${result.durationMs}ms) -> ${result.output || 'stdout'}`);
  }
  if (summary.status !== 'pass') process.exitCode = 1;
}

main().catch((error) => {
  process.stderr.write(`${error?.message || String(error)}\n`);
  process.exitCode = 2;
});
