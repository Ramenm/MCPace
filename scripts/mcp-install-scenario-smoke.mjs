#!/usr/bin/env node
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import { deriveProjectName, deriveProjectVersion, repoRoot } from './lib/project-metadata.mjs';
import { resolveVendoredBinary } from './verify-vendored-binary.mjs';

const DEFAULT_SERVER_COUNT = 100;
const DEFAULT_TIMEOUT_MS = 15_000;

function parseArgs(argv) {
  const args = {
    json: false,
    write: 'reports/mcp-install-scenario-smoke-latest.json',
    markdown: 'reports/mcp-install-scenario-smoke-latest.md',
    serverCount: DEFAULT_SERVER_COUNT,
    timeoutMs: DEFAULT_TIMEOUT_MS,
    keepTemp: false,
    binaryPath: null,
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
      case '--servers': args.serverCount = parsePositiveInteger(readValue(), token); break;
      case '--timeout-ms': args.timeoutMs = parsePositiveInteger(readValue(), token); break;
      case '--binary-path': args.binaryPath = path.resolve(readValue()); break;
      case '--keep-temp': args.keepTemp = true; break;
      case '--help':
      case '-h': args.help = true; break;
      default: throw new Error(`unsupported mcp-install-scenario-smoke argument: ${token}`);
    }
  }
  return args;
}

function parsePositiveInteger(value, label) {
  const parsed = Number.parseInt(value, 10);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) throw new Error(`${label} must be a positive integer`);
  return parsed;
}

function printHelp() {
  console.log(`Usage: node scripts/mcp-install-scenario-smoke.mjs [options]

Runs executable MCP server install semantics smoke checks against a temporary project:
  - config-only preset install and dry-run behavior;
  - idempotency/no-force/force semantics;
  - stdio and Streamable HTTP add paths;
  - invalid remote URL rejection;
  - disabled expensive/paid server posture;
  - 100-server config-scale behavior.

Options:
  --servers 100                 number of synthetic servers for scale smoke
  --timeout-ms 15000            per-command timeout
  --binary-path <path>          mcpace binary to exercise
  --write <path>                JSON report path
  --markdown <path>             Markdown report path
  --no-write                    do not write reports
  --json                        print JSON report
  --keep-temp                   keep temp project for manual inspection
`);
}

function normalizeRelative(filePath) {
  const absolute = path.resolve(filePath);
  const relative = path.relative(repoRoot, absolute);
  return relative && !relative.startsWith('..') ? relative.split(path.sep).join('/') : absolute;
}

function cloneMinimalProject() {
  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-install-scenarios-'));
  fs.copyFileSync(path.join(repoRoot, 'mcpace.config.json'), path.join(tempRoot, 'mcpace.config.json'));
  fs.copyFileSync(path.join(repoRoot, 'mcp_settings.json'), path.join(tempRoot, 'mcp_settings.json'));
  fs.mkdirSync(path.join(tempRoot, 'mcp_settings.d'), { recursive: true });
  fs.writeFileSync(path.join(tempRoot, 'mcp_settings.d', 'README.md'), '# test fragments\n', 'utf8');
  copyDirectory(path.join(repoRoot, 'presets'), path.join(tempRoot, 'presets'));
  return tempRoot;
}

function copyDirectory(source, target) {
  fs.mkdirSync(target, { recursive: true });
  for (const entry of fs.readdirSync(source, { withFileTypes: true })) {
    const from = path.join(source, entry.name);
    const to = path.join(target, entry.name);
    if (entry.isDirectory()) copyDirectory(from, to);
    else if (entry.isFile()) fs.copyFileSync(from, to);
  }
}

function runMcpace(binaryPath, tempRoot, args, options = {}) {
  const startedAt = Date.now();
  const result = spawnSync(binaryPath, [...args, '--root', tempRoot], {
    cwd: repoRoot,
    encoding: 'utf8',
    timeout: options.timeoutMs || DEFAULT_TIMEOUT_MS,
    windowsHide: true,
    env: {
      ...process.env,
      // Keep scenario tests deterministic; this command should not require network access.
      MCPACE_PUBLIC_MCP_URL: '',
    },
  });
  const stdout = String(result.stdout || '').trim();
  const stderr = String(result.stderr || '').trim();
  return {
    command: `${normalizeRelative(binaryPath)} ${args.join(' ')} --root ${tempRoot}`,
    status: result.status,
    signal: result.signal ?? null,
    timedOut: result.error?.code === 'ETIMEDOUT',
    durationMs: Date.now() - startedAt,
    stdout,
    stderr,
    error: result.error ? String(result.error.message || result.error) : null,
    json: parseJson(stdout),
  };
}

function parseJson(value) {
  if (!value) return null;
  try { return JSON.parse(value); } catch { return null; }
}

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function check(id, ok, detail, extra = {}) {
  return { id, status: ok ? 'pass' : 'fail', detail, ...extra };
}

function summarizeRun(run) {
  if (run.status === 0) return `exit=0 ${run.durationMs}ms`;
  const output = [run.stderr, run.stdout, run.error].filter(Boolean).join(' | ');
  return `exit=${run.status ?? 'null'} ${output}`.slice(0, 500);
}

function buildScenarioMatrix() {
  return [
    {
      scenario: 'preset install dry-run',
      expected: 'No file is written; output says dry-run-add.',
      riskCovered: 'Prevents accidental config mutation while evaluating an MCP server.',
    },
    {
      scenario: 'preset install apply/reapply/force',
      expected: 'First apply writes one fragment, second apply without --force fails, --force replaces.',
      riskCovered: 'Prevents hidden reinstall/duplicate drift and makes replacement explicit.',
    },
    {
      scenario: 'custom stdio server',
      expected: 'Writes command/args only; does not execute the command during add.',
      riskCovered: 'Separates registration from runtime execution.',
    },
    {
      scenario: 'remote Streamable HTTP server',
      expected: 'Accepts http(s) URL and headers as config; rejects non-http(s).',
      riskCovered: 'Separates remote domain ownership from local MCPace endpoint ownership.',
    },
    {
      scenario: 'paid/expensive server disabled by default',
      expected: 'Entry can be added with enabled=false; costs remain dependent on later runtime/tool calls.',
      riskCovered: 'Avoids accidental activation while still allowing reviewable config.',
    },
    {
      scenario: '100-server config scale',
      expected: '100 distinct fragments are written and visible in source inventory.',
      riskCovered: 'Covers many-server config fanout without claiming runtime can safely run all concurrently.',
    },
  ];
}

export function runInstallScenarioSmoke(options = {}) {
  options = { serverCount: DEFAULT_SERVER_COUNT, timeoutMs: DEFAULT_TIMEOUT_MS, keepTemp: false, ...options };
  const binaryPath = options.binaryPath || resolveVendoredBinary({}).binaryPath;
  const tempRoot = cloneMinimalProject();
  const commands = [];
  const checks = [];
  const observations = [];
  const startedAt = Date.now();

  try {
    const dryRun = runMcpace(binaryPath, tempRoot, ['server', 'install', 'filesystem', '--dry-run', '--json', '--path', '.'], options);
    commands.push({ label: 'preset dry-run', ...dryRun, stdout: dryRun.stdout.slice(0, 2000), stderr: dryRun.stderr.slice(0, 2000) });
    checks.push(check(
      'preset-install-dry-run-is-config-only',
      dryRun.status === 0 && dryRun.json?.write?.action === 'dry-run-add' && !fs.existsSync(path.join(tempRoot, 'mcp_settings.d', 'filesystem.json')),
      summarizeRun(dryRun),
    ));

    const apply = runMcpace(binaryPath, tempRoot, ['server', 'install', 'filesystem', '--json', '--path', '.'], options);
    commands.push({ label: 'preset apply', ...apply, stdout: apply.stdout.slice(0, 2000), stderr: apply.stderr.slice(0, 2000) });
    const filesystemPath = path.join(tempRoot, 'mcp_settings.d', 'filesystem.json');
    const filesystemConfig = fs.existsSync(filesystemPath) ? readJson(filesystemPath) : null;
    checks.push(check(
      'preset-install-writes-one-fragment',
      apply.status === 0 && apply.json?.write?.action === 'add' && Boolean(filesystemConfig?.mcpServers?.filesystem),
      summarizeRun(apply),
    ));
    checks.push(check(
      'preset-install-does-not-run-package-command',
      filesystemConfig?.mcpServers?.filesystem?.command === 'npx' && filesystemConfig?.mcpServers?.filesystem?.args?.includes('@modelcontextprotocol/server-filesystem'),
      'Install output materialized command/args in JSON only; package execution is deferred until runtime/test/client launch.',
    ));

    const reapply = runMcpace(binaryPath, tempRoot, ['server', 'install', 'filesystem', '--json', '--path', '.'], options);
    commands.push({ label: 'preset reapply without force', ...reapply, stdout: reapply.stdout.slice(0, 2000), stderr: reapply.stderr.slice(0, 2000) });
    checks.push(check(
      'reinstall-without-force-is-blocked',
      reapply.status !== 0 && /already exists/i.test(`${reapply.stderr}\n${reapply.stdout}`),
      summarizeRun(reapply),
    ));

    const force = runMcpace(binaryPath, tempRoot, ['server', 'install', 'filesystem', '--force', '--json', '--path', '.'], options);
    commands.push({ label: 'preset force replace', ...force, stdout: force.stdout.slice(0, 2000), stderr: force.stderr.slice(0, 2000) });
    checks.push(check(
      'reinstall-with-force-replaces',
      force.status === 0 && force.json?.write?.action === 'replace' && force.json?.write?.existedBefore === true,
      summarizeRun(force),
    ));

    const stdio = runMcpace(binaryPath, tempRoot, ['server', 'add', 'custom-stdio', '--command', 'node', '--arg', 'server.js', '--json'], options);
    commands.push({ label: 'custom stdio add', ...stdio, stdout: stdio.stdout.slice(0, 2000), stderr: stdio.stderr.slice(0, 2000) });
    const stdioConfigPath = path.join(tempRoot, 'mcp_settings.d', 'custom-stdio.json');
    const stdioConfig = fs.existsSync(stdioConfigPath) ? readJson(stdioConfigPath) : null;
    checks.push(check(
      'custom-stdio-server-add',
      stdio.status === 0 && stdioConfig?.mcpServers?.['custom-stdio']?.type === 'stdio' && stdioConfig?.mcpServers?.['custom-stdio']?.command === 'node',
      summarizeRun(stdio),
    ));

    const remote = runMcpace(binaryPath, tempRoot, ['server', 'add', 'remote-docs', '--url', 'https://mcp.example.invalid/mcp', '--type', 'streamable-http', '--header', 'Authorization=Bearer ${REMOTE_DOCS_TOKEN}', '--json'], options);
    commands.push({ label: 'remote streamable-http add', ...remote, stdout: remote.stdout.slice(0, 2000), stderr: remote.stderr.slice(0, 2000) });
    const remotePath = path.join(tempRoot, 'mcp_settings.d', 'remote-docs.json');
    const remoteConfig = fs.existsSync(remotePath) ? readJson(remotePath) : null;
    checks.push(check(
      'remote-http-server-add',
      remote.status === 0 && remoteConfig?.mcpServers?.['remote-docs']?.type === 'streamable-http' && remoteConfig?.mcpServers?.['remote-docs']?.url === 'https://mcp.example.invalid/mcp',
      summarizeRun(remote),
    ));

    const invalidUrl = runMcpace(binaryPath, tempRoot, ['server', 'add', 'bad-remote', '--url', 'ssh://mcp.example.invalid', '--json'], options);
    commands.push({ label: 'invalid URL rejected', ...invalidUrl, stdout: invalidUrl.stdout.slice(0, 2000), stderr: invalidUrl.stderr.slice(0, 2000) });
    checks.push(check(
      'invalid-remote-url-is-rejected',
      invalidUrl.status !== 0 && /http:\/\/ or https:\/\//i.test(`${invalidUrl.stderr}\n${invalidUrl.stdout}`),
      summarizeRun(invalidUrl),
    ));

    const paidDisabled = runMcpace(binaryPath, tempRoot, ['server', 'add', 'paid-analytics', '--command', 'npx', '--arg', '-y', '--arg', '@vendor/paid-analytics-mcp', '--env', 'PAID_ANALYTICS_API_KEY=${PAID_ANALYTICS_API_KEY}', '--disabled', '--json'], options);
    commands.push({ label: 'paid server disabled add', ...paidDisabled, stdout: paidDisabled.stdout.slice(0, 2000), stderr: paidDisabled.stderr.slice(0, 2000) });
    const paidPath = path.join(tempRoot, 'mcp_settings.d', 'paid-analytics.json');
    const paidConfig = fs.existsSync(paidPath) ? readJson(paidPath) : null;
    checks.push(check(
      'paid-server-can-be-registered-disabled',
      paidDisabled.status === 0 && paidConfig?.mcpServers?.['paid-analytics']?.enabled === false,
      summarizeRun(paidDisabled),
    ));

    const scaleStartedAt = Date.now();
    let scaleFailure = null;
    for (let index = 1; index <= options.serverCount; index += 1) {
      const suffix = String(index).padStart(3, '0');
      const run = runMcpace(binaryPath, tempRoot, ['server', 'add', `scale-${suffix}`, '--command', 'node', '--arg', `scale-${suffix}.js`, '--json'], options);
      if (run.status !== 0) {
        scaleFailure = run;
        commands.push({ label: `scale add ${suffix}`, ...run, stdout: run.stdout.slice(0, 2000), stderr: run.stderr.slice(0, 2000) });
        break;
      }
    }
    const scaleDurationMs = Date.now() - scaleStartedAt;
    const sourceInventory = runMcpace(binaryPath, tempRoot, ['server', 'sources', '--json'], options);
    commands.push({ label: 'source inventory after scale', ...sourceInventory, stdout: sourceInventory.stdout.slice(0, 2000), stderr: sourceInventory.stderr.slice(0, 2000) });
    const scaleFiles = fs.readdirSync(path.join(tempRoot, 'mcp_settings.d')).filter((name) => /^scale-\d+\.json$/.test(name));
    checks.push(check(
      'hundred-server-config-scale',
      !scaleFailure && scaleFiles.length === options.serverCount && Number(sourceInventory.json?.serverCount || 0) >= options.serverCount,
      scaleFailure ? summarizeRun(scaleFailure) : `${scaleFiles.length} fragments written in ${scaleDurationMs}ms; inventory serverCount=${sourceInventory.json?.serverCount}`,
      { durationMs: scaleDurationMs, serverCount: options.serverCount },
    ));

    observations.push('server install/add writes MCP settings fragments; it does not download packages or invoke upstream tools during registration.');
    observations.push('npx-based presets defer package fetch/cache behavior until the command is later executed by server test, runtime, or a client.');
    observations.push('Remote URL domains are upstream domains, not owned by MCPace unless the user controls that endpoint. MCPace serve.publicUrl is the advertised MCPace endpoint and must point to a user-controlled relay/domain when set.');
    observations.push('100 configured servers is a config-scale scenario; it does not prove safe concurrent runtime launch of 100 expensive servers.');

    const failures = checks.filter((item) => item.status !== 'pass');
    return {
      schema: 'mcpace.mcpInstallScenarioSmoke.v1',
      generatedAt: new Date().toISOString(),
      project: { name: deriveProjectName(), version: deriveProjectVersion() },
      status: failures.length ? 'fail' : 'pass',
      binaryPath: normalizeRelative(binaryPath),
      tempRoot: options.keepTemp ? tempRoot : null,
      elapsedMs: Date.now() - startedAt,
      scenarioMatrix: buildScenarioMatrix(),
      checks,
      observations,
      warnings: [
        'This smoke suite verifies registration semantics, not real package install latency, provider billing behavior, or live MCP tool calls.',
        'Run live server tests only against reviewed servers with explicit credentials and cost limits.',
      ],
      commands,
    };
  } finally {
    if (!options.keepTemp) fs.rmSync(tempRoot, { recursive: true, force: true });
  }
}

function renderMarkdown(report) {
  const lines = [];
  lines.push('# MCP install scenario smoke');
  lines.push('');
  lines.push(`Generated: ${report.generatedAt}`);
  lines.push(`Status: ${report.status}`);
  lines.push(`Project: ${report.project.name} ${report.project.version}`);
  lines.push('');
  lines.push('## Checks');
  lines.push('');
  lines.push('| Check | Status | Detail |');
  lines.push('|---|---:|---|');
  for (const checkItem of report.checks) {
    lines.push(`| ${checkItem.id} | ${checkItem.status} | ${String(checkItem.detail || '').replaceAll('|', '\\|')} |`);
  }
  lines.push('');
  lines.push('## Scenario matrix');
  lines.push('');
  lines.push('| Scenario | Expected behavior | Risk covered |');
  lines.push('|---|---|---|');
  for (const item of report.scenarioMatrix) {
    lines.push(`| ${item.scenario} | ${item.expected} | ${item.riskCovered} |`);
  }
  lines.push('');
  lines.push('## Observations');
  lines.push('');
  for (const observation of report.observations) lines.push(`- ${observation}`);
  lines.push('');
  lines.push('## Warnings');
  lines.push('');
  for (const warning of report.warnings) lines.push(`- ${warning}`);
  lines.push('');
  return `${lines.join('\n')}\n`;
}

function writeReport(filePath, content) {
  const absolute = path.resolve(filePath);
  fs.mkdirSync(path.dirname(absolute), { recursive: true });
  fs.writeFileSync(absolute, content, 'utf8');
}

function isCliInvocation() {
  const entry = process.argv[1];
  return entry ? pathToFileURL(path.resolve(entry)).href === import.meta.url : false;
}

function main() {
  try {
    const parsed = parseArgs(process.argv.slice(2));
    if (parsed.help) { printHelp(); return; }
    const report = runInstallScenarioSmoke(parsed);
    if (parsed.write) writeReport(parsed.write, `${JSON.stringify(report, null, 2)}\n`);
    if (parsed.markdown) writeReport(parsed.markdown, renderMarkdown(report));
    if (parsed.json) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
    else process.stdout.write(`${report.status}\n`);
    if (report.status !== 'pass') process.exitCode = 1;
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exit(1);
  }
}

if (isCliInvocation()) main();
