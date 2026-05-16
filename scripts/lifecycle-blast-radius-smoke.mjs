#!/usr/bin/env node
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import { deriveProjectName, deriveProjectVersion, repoRoot } from './lib/project-metadata.mjs';
import { resolveVendoredBinary } from './verify-vendored-binary.mjs';

const DEFAULT_TIMEOUT_MS = 15_000;
const SECRET_SENTINEL = 'sk_live_must_not_appear_in_command_output_123';

function parseArgs(argv) {
  const args = {
    json: false,
    write: 'reports/lifecycle-blast-radius-latest.json',
    markdown: 'reports/lifecycle-blast-radius-latest.md',
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
      case '--timeout-ms': args.timeoutMs = parsePositiveInteger(readValue(), token); break;
      case '--binary-path': args.binaryPath = path.resolve(readValue()); break;
      case '--keep-temp': args.keepTemp = true; break;
      case '--help':
      case '-h': args.help = true; break;
      default: throw new Error(`unsupported lifecycle-blast-radius-smoke argument: ${token}`);
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
  console.log(`Usage: node scripts/lifecycle-blast-radius-smoke.mjs [options]\n\nChecks high-risk MCP server lifecycle and blast-radius behavior:\n  - disabled paid server registration;\n  - enable/disable/remove/re-add idempotency;\n  - secret values not echoed in CLI output;\n  - source-level corrupt-fragment isolation and normalized duplicate replacement guards;\n  - owned vs upstream and supply-chain documentation coverage.\n\nOptions:\n  --timeout-ms 15000            per-command timeout\n  --binary-path <path>          mcpace binary to exercise for executable lifecycle checks\n  --write <path>                JSON report path\n  --markdown <path>             Markdown report path\n  --no-write                    do not write reports\n  --json                        print JSON report\n  --keep-temp                   keep temp project for manual inspection\n`);
}

function normalizeRelative(filePath) {
  const absolute = path.resolve(filePath);
  const relative = path.relative(repoRoot, absolute);
  return relative && !relative.startsWith('..') ? relative.split(path.sep).join('/') : absolute;
}

function cloneMinimalProject() {
  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-lifecycle-blast-'));
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
    env: { ...process.env, MCPACE_PUBLIC_MCP_URL: '' },
  });
  const stdout = String(result.stdout || '').trim();
  const stderr = String(result.stderr || '').trim();
  return {
    command: redactSecrets(`${normalizeRelative(binaryPath)} ${args.join(' ')} --root ${tempRoot}`),
    status: result.status,
    signal: result.signal ?? null,
    timedOut: result.error?.code === 'ETIMEDOUT',
    durationMs: Date.now() - startedAt,
    stdout: redactSecrets(stdout),
    stderr: redactSecrets(stderr),
    leakedSecret: stdout.includes(SECRET_SENTINEL) || stderr.includes(SECRET_SENTINEL),
    error: result.error ? redactSecrets(String(result.error.message || result.error)) : null,
    json: parseJson(stdout),
  };
}

function redactSecrets(value) {
  return String(value || '').replaceAll(SECRET_SENTINEL, '[REDACTED_SECRET_SENTINEL]');
}

function parseJson(value) {
  if (!value) return null;
  try { return JSON.parse(value); } catch { return null; }
}

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function readText(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

function check(id, ok, detail, extra = {}) {
  return { id, ok: Boolean(ok), status: ok ? 'pass' : 'fail', detail, ...extra };
}

function summarizeRun(run) {
  if (run.status === 0) return `exit=0 ${run.durationMs}ms`;
  const output = [run.stderr, run.stdout, run.error].filter(Boolean).join(' | ');
  return `exit=${run.status ?? 'null'} ${output}`.slice(0, 500);
}

function assertSourceHardeningChecks(checks) {
  const writeSource = readText('src/mcp_sources/write.rs');
  const registrySource = readText('src/mcp_sources.rs');
  const installDocs = readText('docs/mcp-server-install-scenarios.md');
  const lifecycleDocs = readText('docs/mcp-lifecycle-blast-radius.md');
  const securityDocs = readText('docs/tool-exposure-and-call-safety.md');
  const presets = readJson('presets/mcp-servers.json');

  checks.push(check(
    'source-force-replace-removes-normalized-duplicate-key',
    /let existing_key = servers[\s\S]+servers\.remove\(&existing_key\)[\s\S]+servers\.insert\(display_name, entry\)/.test(writeSource),
    'Force replace removes an existing normalized-match key before inserting the replacement key.',
  ));
  checks.push(check(
    'source-corrupt-settings-fragment-isolated',
    /failed to read MCP settings source[\s\S]+skipping[\s\S]+continue;/.test(registrySource),
    'Registry/source-report loaders skip unreadable JSON sources with warnings instead of failing the entire registry.',
  ));
  checks.push(check(
    'docs-distinguish-owned-and-upstream-domains',
    /Owned by MCPace[\s\S]+Not owned by MCPace/.test(installDocs) && /upstream MCP server domain/i.test(installDocs),
    'Install docs distinguish local MCPace ownership from upstream package/domain/provider ownership.',
  ));
  checks.push(check(
    'docs-blast-radius-require-disabled-review-consent',
    /registered but disabled/i.test(lifecycleDocs) && /explicit enable/i.test(lifecycleDocs) && /consent/i.test(lifecycleDocs),
    'Lifecycle docs require paid/risky servers to be registered disabled, then explicitly enabled/consented.',
  ));
  checks.push(check(
    'docs-tool-safety-covers-arbitrary-code-execution',
    /arbitrary code execution/i.test(securityDocs) && /explicit consent/i.test(securityDocs),
    'Tool safety docs treat MCP tools as arbitrary-code/data-access surfaces with explicit consent.',
  ));

  const unpinnedLaunchers = [];
  for (const preset of presets.presets || []) {
    if (!['npx', 'uvx', 'docker'].includes(String(preset.command || '').toLowerCase())) continue;
    const args = Array.isArray(preset.args) ? preset.args : [];
    const joined = args.join(' ');
    const pinned = /@\d+\.\d+\.\d+/.test(joined) || /@sha256:/.test(joined) || /--from\s+[^\s]+==\d+\.\d+\.\d+/.test(joined);
    if (!pinned) unpinnedLaunchers.push({ id: preset.id, command: preset.command, args });
  }
  checks.push(check(
    'supply-chain-unpinned-launchers-are-documented-risk',
    unpinnedLaunchers.length > 0 && /unpinned|package manager|cache miss|typosquat/i.test(lifecycleDocs),
    `${unpinnedLaunchers.length} launcher presets are unpinned; docs must explicitly call out package-manager risk.`,
    { unpinnedLaunchers },
  ));
}

function buildReport(options = {}) {
  options = { timeoutMs: DEFAULT_TIMEOUT_MS, keepTemp: false, ...options };
  const binaryPath = options.binaryPath || resolveVendoredBinary({}).binaryPath;
  const tempRoot = cloneMinimalProject();
  const commands = [];
  const checks = [];
  const observations = [];
  const startedAt = Date.now();

  try {
    const paidAdd = runMcpace(binaryPath, tempRoot, [
      'server', 'add', 'paid-billing',
      '--command', 'npx',
      '--arg', '-y',
      '--arg', '@vendor/paid-billing-mcp',
      '--env', `PAID_BILLING_API_KEY=${SECRET_SENTINEL}`,
      '--disabled',
      '--json',
    ], options);
    commands.push({ label: 'register paid server disabled', ...paidAdd });
    const paidPath = path.join(tempRoot, 'mcp_settings.d', 'paid-billing.json');
    const paidConfig = fs.existsSync(paidPath) ? readJson(paidPath) : null;
    checks.push(check(
      'paid-server-registers-disabled-without-output-secret-leak',
      paidAdd.status === 0 && paidConfig?.mcpServers?.['paid-billing']?.enabled === false && !paidAdd.leakedSecret,
      summarizeRun(paidAdd),
    ));

    const enable = runMcpace(binaryPath, tempRoot, ['server', 'enable', 'paid-billing', '--json'], options);
    commands.push({ label: 'enable paid server explicitly', ...enable });
    const enabledConfig = fs.existsSync(paidPath) ? readJson(paidPath) : null;
    checks.push(check(
      'server-enable-is-explicit-state-transition',
      enable.status === 0 && enabledConfig?.mcpServers?.['paid-billing']?.enabled === true && enable.json?.previousEnabled === false,
      summarizeRun(enable),
    ));

    const disable = runMcpace(binaryPath, tempRoot, ['server', 'disable', 'paid-billing', '--json'], options);
    commands.push({ label: 'disable paid server explicitly', ...disable });
    const disabledConfig = fs.existsSync(paidPath) ? readJson(paidPath) : null;
    checks.push(check(
      'server-disable-is-explicit-state-transition',
      disable.status === 0 && disabledConfig?.mcpServers?.['paid-billing']?.enabled === false && disable.json?.previousEnabled === true,
      summarizeRun(disable),
    ));

    const dryRemove = runMcpace(binaryPath, tempRoot, ['server', 'remove', 'paid-billing', '--dry-run', '--json'], options);
    commands.push({ label: 'remove paid server dry-run', ...dryRemove });
    checks.push(check(
      'server-remove-dry-run-does-not-delete',
      dryRemove.status === 0 && fs.existsSync(paidPath) && Boolean(readJson(paidPath).mcpServers?.['paid-billing']),
      summarizeRun(dryRemove),
    ));

    const remove = runMcpace(binaryPath, tempRoot, ['server', 'remove', 'paid-billing', '--json'], options);
    commands.push({ label: 'remove paid server', ...remove });
    const afterRemove = fs.existsSync(paidPath) ? readJson(paidPath) : null;
    checks.push(check(
      'server-remove-deletes-only-target-entry',
      remove.status === 0 && !afterRemove?.mcpServers?.['paid-billing'],
      summarizeRun(remove),
    ));

    const readd = runMcpace(binaryPath, tempRoot, ['server', 'add', 'paid-billing', '--command', 'node', '--arg', 'safe-stub.js', '--disabled', '--json'], options);
    commands.push({ label: 're-add after remove', ...readd });
    checks.push(check(
      'server-can-be-readded-after-remove',
      readd.status === 0 && readJson(paidPath).mcpServers?.['paid-billing']?.command === 'node',
      summarizeRun(readd),
    ));

    const duplicate = runMcpace(binaryPath, tempRoot, ['server', 'add', 'Paid Billing', '--command', 'node', '--arg', 'duplicate.js', '--json'], options);
    commands.push({ label: 'normalized duplicate blocked', ...duplicate });
    checks.push(check(
      'normalized-duplicate-without-force-blocked',
      duplicate.status !== 0 && /already exists/i.test(`${duplicate.stderr}\n${duplicate.stdout}`),
      summarizeRun(duplicate),
    ));

    assertSourceHardeningChecks(checks);

    observations.push('Executable lifecycle checks exercise the vendored binary available in this source archive; source-only hardening checks cover Rust changes that still need cargo/rustc proof.');
    observations.push('Paid/risky servers should stay disabled through registration and only become active through an explicit enable/consent transition.');
    observations.push('Package-manager launchers such as npx/uvx/docker remain upstream/supply-chain surfaces; this smoke does not execute those packages.');
    observations.push('Corrupt-fragment isolation and normalized duplicate replacement were patched at source level; they are not release-proven until a Rust host rebuilds the binary and runs the Rust lanes.');

    const failures = checks.filter((item) => !item.ok);
    return {
      schema: 'mcpace.lifecycleBlastRadiusSmoke.v1',
      generatedAt: new Date().toISOString(),
      project: { name: deriveProjectName(), version: deriveProjectVersion() },
      status: failures.length ? 'fail' : 'pass',
      binaryPath: normalizeRelative(binaryPath),
      tempRoot: options.keepTemp ? tempRoot : null,
      elapsedMs: Date.now() - startedAt,
      checks,
      observations,
      warnings: [
        'This smoke suite does not execute remote paid tools, npx packages, uvx packages, Docker images, or browser automation packages.',
        'Rust source changes remain source-checked only in this sandbox because cargo/rustc are unavailable here.',
        'A real release gate must rebuild the vendored binary and rerun this smoke against the rebuilt artifact.',
      ],
      commands,
    };
  } finally {
    if (!options.keepTemp) fs.rmSync(tempRoot, { recursive: true, force: true });
  }
}

function renderMarkdown(report) {
  const lines = [];
  lines.push('# Lifecycle and blast-radius smoke');
  lines.push('');
  lines.push(`Generated: ${report.generatedAt}`);
  lines.push(`Status: ${report.status}`);
  lines.push(`Project: ${report.project.name} ${report.project.version}`);
  lines.push('');
  lines.push('## Checks');
  lines.push('');
  lines.push('| Check | Status | Detail |');
  lines.push('|---|---:|---|');
  for (const item of report.checks) {
    lines.push(`| ${item.id} | ${item.status} | ${String(item.detail || '').replaceAll('|', '\\|')} |`);
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
    const report = buildReport(parsed);
    if (parsed.write) writeReport(parsed.write, `${JSON.stringify(report, null, 2)}\n`);
    if (parsed.markdown) writeReport(parsed.markdown, renderMarkdown(report));
    process.stdout.write(parsed.json ? `${JSON.stringify(report, null, 2)}\n` : `${report.status}\n`);
    if (report.status !== 'pass') process.exitCode = 1;
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exit(1);
  }
}

if (isCliInvocation()) main();

export { buildReport, renderMarkdown };
