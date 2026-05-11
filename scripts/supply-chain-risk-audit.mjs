#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { deriveProjectName, deriveProjectVersion, readJson, repoRoot } from './lib/project-metadata.mjs';
import { childEnvForCommand } from './lib/safe-child-env.mjs';

function parseArgs(argv) {
  const out = { json: false, write: null, markdown: null, strict: false, timeoutMs: 30_000, help: false };
  for (let i = 0; i < argv.length; i += 1) {
    const a = argv[i];
    if (a === '--json') out.json = true;
    else if (a === '--write') out.write = argv[++i] || null;
    else if (a === '--markdown' || a === '--write-md') out.markdown = argv[++i] || null;
    else if (a === '--strict') out.strict = true;
    else if (a === '--timeout-ms') out.timeoutMs = Number(argv[++i] || 0);
    else if (a === '-h' || a === '--help') out.help = true;
    else throw new Error(`unsupported supply-chain-risk-audit argument: ${a}`);
  }
  if (!Number.isSafeInteger(out.timeoutMs) || out.timeoutMs <= 0) out.timeoutMs = 30_000;
  return out;
}

function status(id, severity, ok, evidence, nextAction = null, details = null) {
  return { id, severity, status: ok ? 'pass' : severity === 'required' ? 'block' : 'warn', evidence, nextAction, details };
}

function run(command, args, opts) {
  const result = spawnSync(command, args, {
    cwd: repoRoot,
    env: childEnvForCommand(command),
    encoding: 'utf8',
    timeout: opts.timeoutMs,
    maxBuffer: 2 * 1024 * 1024,
    windowsHide: true,
  });
  return {
    command: [command, ...args].join(' '),
    ok: result.status === 0 && !result.error,
    code: result.status,
    signal: result.signal ?? null,
    stdout: String(result.stdout || '').split(/\r?\n/).filter(Boolean).slice(-20).join('\n'),
    stderr: String(result.stderr || '').split(/\r?\n/).filter(Boolean).slice(-20).join('\n'),
    error: result.error ? String(result.error.message || result.error) : null,
  };
}

function commandAvailable(command, args, opts) {
  const probe = run(command, args, opts);
  return { available: probe.ok, probe };
}

function dependencyPolicyChecks() {
  const rootPkg = readJson('package.json');
  const cliPkg = readJson('packages/npm/cli/package.json');
  const checks = [];
  const rootDeps = Object.keys(rootPkg.dependencies || {});
  const rootDevDeps = Object.keys(rootPkg.devDependencies || {});
  checks.push(status('root-package-no-runtime-deps', 'required', rootDeps.length === 0, rootDeps.length ? `root dependencies present: ${rootDeps.join(', ')}` : 'root package has no runtime dependencies', 'Keep the root package as an automation workspace without runtime npm dependencies.'));
  checks.push(status('root-package-no-dev-deps', 'advisory', rootDevDeps.length === 0, rootDevDeps.length ? `root devDependencies present: ${rootDevDeps.join(', ')}` : 'root package has no devDependencies; scripts use built-in Node/Rust tooling', 'This is not required, but keeping the automation dependency-light reduces supply-chain risk.'));
  const cliDeps = Object.keys(cliPkg.dependencies || {});
  const cliOptional = Object.keys(cliPkg.optionalDependencies || {});
  checks.push(status('cli-launcher-no-runtime-deps', 'required', cliDeps.length === 0, cliDeps.length ? `CLI dependencies present: ${cliDeps.join(', ')}` : 'thin npm launcher has no runtime dependencies', 'Keep @mcpace/cli dependency-free unless a dependency is unavoidable.'));
  checks.push(status('platform-optional-dependencies', 'required', cliOptional.length === 6, `${cliOptional.length} platform optional dependencies`, 'Keep platform package matrix complete and version-aligned.'));
  checks.push(status('cargo-lock-present', 'required', fs.existsSync(path.join(repoRoot, 'Cargo.lock')), 'Cargo.lock present', 'Commit Cargo.lock for reproducible binary builds.'));
  checks.push(status('package-lock-absent-ok', 'advisory', !fs.existsSync(path.join(repoRoot, 'package-lock.json')), fs.existsSync(path.join(repoRoot, 'package-lock.json')) ? 'package-lock.json present' : 'no package-lock.json because the workspace has no npm deps', 'If npm dependencies are added, commit and audit the lockfile.'));
  return checks;
}

function optionalToolChecks(opts) {
  const checks = [];
  const cargoAudit = commandAvailable('cargo', ['audit', '--version'], opts);
  if (cargoAudit.available) {
    const audit = run('cargo', ['audit'], opts);
    checks.push(status('cargo-audit', 'recommended', audit.ok, audit.ok ? 'cargo audit passed' : 'cargo audit failed or found advisories', 'Review RustSec advisories before publishing.', audit));
  } else {
    checks.push(status('cargo-audit', 'recommended', false, 'cargo-audit not installed/available', 'Install cargo-audit and run cargo audit before public release.', cargoAudit.probe));
  }
  const cargoDeny = commandAvailable('cargo', ['deny', '--version'], opts);
  if (cargoDeny.available) {
    const deny = run('cargo', ['deny', 'check'], opts);
    checks.push(status('cargo-deny', 'recommended', deny.ok, deny.ok ? 'cargo deny check passed' : 'cargo deny check failed or needs policy/config', 'Review dependency/license/source policy before publishing.', deny));
  } else {
    checks.push(status('cargo-deny', 'recommended', false, 'cargo-deny not installed/available', 'Install cargo-deny for license/source/advisory policy before public release.', cargoDeny.probe));
  }
  for (const [id, command, args, hint] of [
    ['gitleaks', 'gitleaks', ['version'], 'Install gitleaks for an independent local secret scan.'],
    ['osv-scanner', 'osv-scanner', ['--version'], 'Install osv-scanner for an independent vulnerability scan.'],
    ['trivy', 'trivy', ['--version'], 'Install trivy if you want container/filesystem vulnerability scans.'],
  ]) {
    const probe = commandAvailable(command, args, opts);
    checks.push(status(id, 'optional', probe.available, probe.available ? `${command} available` : `${command} not installed/available`, hint, probe.probe));
  }
  return checks;
}

function buildReport(opts) {
  const checks = [...dependencyPolicyChecks(), ...optionalToolChecks(opts)];
  const blockers = checks.filter((c) => c.status === 'block');
  const warnings = checks.filter((c) => c.status === 'warn');
  return {
    schema: 'mcpace.supplyChainRisk.v1',
    generatedAt: new Date().toISOString(),
    project: { name: deriveProjectName(), version: deriveProjectVersion() },
    status: blockers.length ? 'blocked' : warnings.length ? 'pass-with-warnings' : 'pass',
    localOnly: true,
    githubPaidPlanRequired: false,
    summary: { total: checks.length, passed: checks.filter((c) => c.status === 'pass').length, warnings: warnings.length, blockers: blockers.length },
    checks,
    nextActions: [...blockers, ...warnings].map((c) => c.nextAction).filter(Boolean),
  };
}

function renderMarkdown(report) {
  return [
    '# MCPace local supply-chain risk audit', '',
    `Project: \`${report.project.name}\` v\`${report.project.version}\``,
    `Status: \`${report.status}\``,
    `GitHub paid plan required: \`${report.githubPaidPlanRequired ? 'yes' : 'no'}\``,
    '', '| check | severity | status | evidence |', '|---|---:|---:|---|',
    ...report.checks.map((c) => `| ${c.id} | ${c.severity} | ${c.status} | ${String(c.evidence || '').replace(/\|/g, '\\|')} |`),
    report.nextActions.length ? '\n## Next actions\n' : '',
    ...report.nextActions.map((action) => `- ${action}`), '',
  ].filter((line) => line !== '').join('\n');
}

function writeArtifacts(report, opts) {
  for (const [file, contents] of [[opts.write, `${JSON.stringify(report, null, 2)}\n`], [opts.markdown, renderMarkdown(report)]]) {
    if (!file) continue;
    const target = path.resolve(repoRoot, file);
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, contents, 'utf8');
  }
}

function main() {
  const opts = parseArgs(process.argv.slice(2));
  if (opts.help) {
    process.stdout.write('Usage: node scripts/supply-chain-risk-audit.mjs [--json] [--write <path>] [--markdown <path>] [--strict]\n');
    return;
  }
  const report = buildReport(opts);
  writeArtifacts(report, opts);
  process.stdout.write(opts.json ? `${JSON.stringify(report, null, 2)}\n` : renderMarkdown(report));
  if (report.summary.blockers > 0 || (opts.strict && report.summary.warnings > 0)) process.exitCode = 1;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { main(); } catch (err) { process.stderr.write(`${err.message || err}\n`); process.exitCode = 1; }
}

export { buildReport, renderMarkdown };
