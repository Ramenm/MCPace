#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import { deriveProjectName, deriveProjectVersion, readJson, repoRoot } from './lib/project-metadata.mjs';
import { cleanChildEnv } from './lib/safe-child-env.mjs';
import { verifyNpmPack } from './verify-npm-pack.mjs';
import { inventorySource } from './inventory-source.mjs';
import { runSyntaxCheck } from './check-node-syntax.mjs';
import { PLATFORM_PACKAGE_TARGETS, platformPackageBinPath } from './lib/npm-platform-packages.mjs';

const DEFAULT_TIMEOUT_MS = 120000;

function parseArgs(argv) {
  const parsed = { json: false, write: null, markdown: null, strict: false, skipNpmPack: false, skipNodeSyntax: false, skipSourceAudit: false, timeoutMs: DEFAULT_TIMEOUT_MS, help: false };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--json': parsed.json = true; break;
      case '--write': parsed.write = argv[++index] || null; if (!parsed.write) throw new Error('boot-harness requires a path after --write'); break;
      case '--write-md':
      case '--markdown': parsed.markdown = argv[++index] || null; if (!parsed.markdown) throw new Error('boot-harness requires a path after --markdown'); break;
      case '--strict': parsed.strict = true; break;
      case '--skip-npm-pack': parsed.skipNpmPack = true; break;
      case '--skip-node-syntax': parsed.skipNodeSyntax = true; break;
      case '--skip-source-audit': parsed.skipSourceAudit = true; break;
      case '--timeout-ms': {
        const value = Number.parseInt(argv[++index] || '', 10);
        if (!Number.isFinite(value) || value < 1000) throw new Error('boot-harness requires --timeout-ms >= 1000');
        parsed.timeoutMs = value;
        break;
      }
      case '-h':
      case '--help': parsed.help = true; break;
      default: throw new Error(`unsupported boot-harness argument: ${token}`);
    }
  }
  return parsed;
}

function printHelp() {
  process.stdout.write('Usage: node scripts/boot-harness.mjs [--json] [--write <path>] [--markdown <path>] [--strict] [--skip-npm-pack] [--skip-node-syntax] [--skip-source-audit]\n\nRuns deterministic install/readiness probes for the source tree.\n');
}

function commandExists(command, args = ['--version'], timeoutMs = 15000) {
  const invocation = commandInvocation(command, args);
  const result = spawnSync(invocation.command, invocation.args, { cwd: repoRoot, encoding: 'utf8', env: cleanChildEnv(), timeout: timeoutMs, windowsHide: true });
  const text = [result.stdout, result.stderr].filter(Boolean).join('\n').trim().split(/\r?\n/)[0] || null;
  return { available: result.status === 0, status: result.status, signal: result.signal || null, versionText: text, error: result.error ? result.error.message : null };
}

function commandInvocation(command, args) {
  if (process.platform === 'win32' && command === 'npm') {
    return { command: 'cmd.exe', args: ['/d', '/s', '/c', 'npm', ...args] };
  }
  return { command, args };
}

function parseMajor(versionText) {
  const match = String(versionText || '').match(/(\d+)\./);
  return match ? Number.parseInt(match[1], 10) : null;
}

function collectToolchain() {
  const pkg = readJson('package.json');
  const engines = pkg.engines || {};
  const nodeVersion = process.version;
  const npm = commandExists('npm', ['--version']);
  const cargo = commandExists('cargo', ['--version']);
  const rustc = commandExists('rustc', ['--version']);
  const nodeMajor = parseMajor(nodeVersion);
  const npmMajor = parseMajor(npm.versionText);
  return {
    engines,
    node: { version: nodeVersion, major: nodeMajor, supported: Number.isFinite(nodeMajor) ? nodeMajor >= 22 : false },
    npm: { ...npm, major: npmMajor, supported: Number.isFinite(npmMajor) ? npmMajor >= 10 : false },
    rust: { cargo, rustc, available: cargo.available && rustc.available }
  };
}

function runSourceAudit(options) {
  if (options.skipSourceAudit) return { status: 'skipped', reason: '--skip-source-audit' };
  const result = spawnSync('node', ['scripts/audit-source.mjs', '--json', '--fail-on-critical'], {
    cwd: repoRoot,
    encoding: 'utf8',
    env: cleanChildEnv(),
    timeout: options.timeoutMs,
    windowsHide: true
  });
  let parsed = null;
  try { parsed = result.stdout ? JSON.parse(result.stdout) : null; } catch { parsed = null; }
  return {
    status: result.status === 0 && parsed?.ok !== false ? 'pass' : 'fail',
    command: 'node scripts/audit-source.mjs --json --fail-on-critical',
    exitCode: result.status,
    signal: result.signal || null,
    error: result.error ? result.error.message : null,
    report: parsed,
    output: [result.stdout, result.stderr].filter(Boolean).join('\n').trim().split(/\r?\n/).slice(0, 25).join('\n') || null
  };
}


function runNodeSyntax(options) {
  if (options.skipNodeSyntax) return { status: 'skipped', reason: '--skip-node-syntax' };
  try { return runSyntaxCheck({}); } catch (error) { return { status: 'fail', reason: error instanceof Error ? error.message : String(error) }; }
}

function collectNpmPack(options) {
  if (options.skipNpmPack) return { status: 'skipped', reason: '--skip-npm-pack' };
  try { return verifyNpmPack({}); } catch (error) { return { status: 'fail', reason: error instanceof Error ? error.message : String(error) }; }
}

function collectBinaryDistribution() {
  const vendorRoot = path.join(repoRoot, 'packages', 'npm', 'cli', 'vendor');
  const vendoredFiles = [];
  if (fs.existsSync(vendorRoot)) {
    const walk = (dir) => {
      for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
        const full = path.join(dir, entry.name);
        if (entry.isDirectory()) walk(full);
        else if (entry.isFile()) vendoredFiles.push(path.relative(vendorRoot, full).split(path.sep).join('/'));
      }
    };
    walk(vendorRoot);
  }
  const platformPackages = PLATFORM_PACKAGE_TARGETS.map((target) => {
    const binaryPath = platformPackageBinPath(target);
    return { key: target.key, packageName: target.packageName, binary: path.relative(repoRoot, binaryPath).split(path.sep).join('/'), present: fs.existsSync(binaryPath) };
  });
  const platformBinaryCount = platformPackages.filter((entry) => entry.present).length;
  const mode = vendoredFiles.length > 0 ? 'vendored-binary-bundle' : platformBinaryCount > 0 ? 'platform-binary-packages' : 'thin-launcher';
  return { mode, readyForPublishedInstall: vendoredFiles.length > 0 || platformBinaryCount > 0, vendoredFiles: vendoredFiles.sort(), platformPackages, platformBinaryCount };
}

function readinessStatus(parts, strict = false) {
  const warnings = [];
  const blockers = [];
  if (!parts.inventory.ok) blockers.push(...parts.inventory.warnings);
  if (parts.sourceAudit.status === 'fail') blockers.push('source audit failed');
  if (parts.nodeSyntax.status === 'fail') blockers.push('node syntax check failed');
  if (parts.npmPack.status === 'fail') blockers.push(`npm pack failed: ${parts.npmPack.reason || 'unknown reason'}`);
  if (!parts.toolchain.node.supported) warnings.push(`current Node ${parts.toolchain.node.version} is below project policy ${parts.toolchain.engines.node || '>=22'}`);
  if (!parts.toolchain.npm.supported) warnings.push(`current npm ${parts.toolchain.npm.versionText || 'missing'} is below project policy ${parts.toolchain.engines.npm || '>=10'}`);
  if (!parts.toolchain.rust.available) warnings.push('cargo/rustc is not fully available in this environment');
  if (!parts.binaryDistribution.readyForPublishedInstall) warnings.push('no vendored/platform native binary staged; npm package remains a thin launcher/source-install artifact');
  const status = blockers.length > 0 ? 'blocked' : strict && warnings.length > 0 ? 'blocked' : warnings.length > 0 ? 'partial' : 'pass';
  return { status, blockers, warnings };
}

function nextActionsFor(report) {
  const actions = [];
  if (!report.toolchain.node.supported || !report.toolchain.npm.supported) actions.push('Use Node 22+ and npm 10+ for official source proof and install checks.');
  actions.push('Run `cargo check --all-targets --locked` and `cargo test --all-targets --locked` on a host with Cargo dependency access.');
  if (!report.binaryDistribution.readyForPublishedInstall) actions.push('Stage a native binary with `node scripts/stage-platform-package-binary.mjs ...` before claiming published npm install readiness.');
  actions.push('Record a real runtime trace: client -> /mcp -> initialize -> tools/list -> tools/call -> stdio upstream response.');
  return actions;
}

export function runBootHarness(options = {}) {
  const toolchain = collectToolchain();
  const inventory = inventorySource({ top: 25 });
  const sourceAudit = runSourceAudit(options);
  const nodeSyntax = runNodeSyntax(options);
  const npmPack = collectNpmPack(options);
  const binaryDistribution = collectBinaryDistribution();
  const partial = { toolchain, inventory, sourceAudit, nodeSyntax, npmPack, binaryDistribution };
  const installReadiness = readinessStatus(partial, Boolean(options.strict));
  const report = {
    schema: 'mcpace.bootHarness.v1',
    generatedAt: new Date().toISOString(),
    project: { name: deriveProjectName(), version: deriveProjectVersion() },
    toolchain,
    inventory,
    sourceAudit,
    nodeSyntax,
    npmPack,
    binaryDistribution,
    installReadiness,
    nextActions: []
  };
  report.nextActions = nextActionsFor(report);
  return report;
}

export function renderBootHarnessMarkdown(report) {
  const lines = ['# MCPace boot harness','',`Generated: ${report.generatedAt}`,'',`Project: \`${report.project.name}\` v\`${report.project.version}\``,'',`Install readiness: **${report.installReadiness.status}**`,'','## Toolchain','','| tool | value | supported |','|---|---|---|',`| node | ${report.toolchain.node.version} | ${report.toolchain.node.supported ? 'yes' : 'no'} |`,`| npm | ${report.toolchain.npm.versionText || 'missing'} | ${report.toolchain.npm.supported ? 'yes' : 'no'} |`,`| cargo | ${report.toolchain.rust.cargo.versionText || 'missing'} | ${report.toolchain.rust.cargo.available ? 'yes' : 'no'} |`,`| rustc | ${report.toolchain.rust.rustc.versionText || 'missing'} | ${report.toolchain.rust.rustc.available ? 'yes' : 'no'} |`,'','## Checks','','| check | status |','|---|---|',`| source inventory | ${report.inventory.ok ? 'pass' : 'attention'} |`,`| source audit | ${report.sourceAudit.status} |`,`| node syntax | ${report.nodeSyntax.status} |`,`| npm pack | ${report.npmPack.status} |`,`| binary distribution | ${report.binaryDistribution.mode} |`,''];
  if (report.installReadiness.blockers.length > 0) { lines.push('## Blockers',''); for (const blocker of report.installReadiness.blockers) lines.push(`- ${blocker}`); lines.push(''); }
  if (report.installReadiness.warnings.length > 0) { lines.push('## Warnings',''); for (const warning of report.installReadiness.warnings) lines.push(`- ${warning}`); lines.push(''); }
  lines.push('## Next actions',''); for (const action of report.nextActions) lines.push(`- ${action}`);
  return `${lines.join('\n')}\n`;
}

function writeFileEnsuringDir(filePath, contents) { const target = path.resolve(filePath); fs.mkdirSync(path.dirname(target), { recursive: true }); fs.writeFileSync(target, contents, 'utf8'); }
function isCliInvocation() { const entry = process.argv[1]; return entry ? pathToFileURL(path.resolve(entry)).href === import.meta.url : false; }
function main() {
  try {
    const parsed = parseArgs(process.argv.slice(2));
    if (parsed.help) { printHelp(); return; }
    const report = runBootHarness(parsed);
    if (parsed.write) writeFileEnsuringDir(parsed.write, `${JSON.stringify(report, null, 2)}\n`);
    if (parsed.markdown) writeFileEnsuringDir(parsed.markdown, renderBootHarnessMarkdown(report));
    if (parsed.json) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
    else {
      process.stdout.write(`install readiness: ${report.installReadiness.status}\n`);
      for (const warning of report.installReadiness.warnings) process.stdout.write(`warning: ${warning}\n`);
      for (const blocker of report.installReadiness.blockers) process.stdout.write(`blocker: ${blocker}\n`);
    }
    if (parsed.strict && report.installReadiness.status !== 'pass') process.exit(1);
  } catch (error) { process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`); process.exit(1); }
}
if (isCliInvocation()) main();
