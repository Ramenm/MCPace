#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { deriveProjectName, deriveProjectVersion, readJson, readText, repoRoot } from './lib/project-metadata.mjs';
import { childEnvForCommand, cleanChildEnv } from './lib/safe-child-env.mjs';

const DEFAULT_TIMEOUT_MS = 7_500;

function parseArgs(argv) {
  const out = { json: false, write: null, markdown: null, strict: false, timeoutMs: DEFAULT_TIMEOUT_MS, help: false };
  for (let i = 0; i < argv.length; i += 1) {
    const a = argv[i];
    if (a === '--json') out.json = true;
    else if (a === '--write') out.write = required(argv[++i], '--write');
    else if (a === '--markdown' || a === '--write-md') out.markdown = required(argv[++i], a);
    else if (a === '--strict') out.strict = true;
    else if (a === '--timeout-ms') out.timeoutMs = positiveInt(argv[++i], '--timeout-ms');
    else if (a === '-h' || a === '--help') out.help = true;
    else throw new Error(`unsupported tooling-readiness argument: ${a}`);
  }
  return out;
}

function required(value, flag) {
  if (!value) throw new Error(`tooling-readiness requires a value after ${flag}`);
  return value;
}

function positiveInt(value, flag) {
  if (!/^\d+$/.test(String(value || ''))) throw new Error(`${flag} must be a positive integer`);
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) throw new Error(`${flag} must be a positive integer`);
  return parsed;
}

function firstLine(text) {
  return String(text || '').split(/\r?\n/).map((line) => line.trim()).find(Boolean) || null;
}

function versionTriple(text) {
  const m = String(text || '').match(/(\d+)\.(\d+)\.(\d+)/);
  return m ? [Number(m[1]), Number(m[2]), Number(m[3])] : null;
}

function versionAtLeast(actualText, minimumText) {
  const actual = versionTriple(actualText);
  const minimum = versionTriple(minimumText);
  if (!actual || !minimum) return false;
  for (let i = 0; i < 3; i += 1) {
    if (actual[i] > minimum[i]) return true;
    if (actual[i] < minimum[i]) return false;
  }
  return true;
}

function invocation(command, args) {
  if (process.platform === 'win32' && ['npm', 'npx', 'cargo', 'rustc', 'git'].includes(command)) {
    return { bin: 'cmd.exe', args: ['/d', '/s', '/c', command, ...args], display: [command, ...args].join(' ') };
  }
  return { bin: command, args, display: [command, ...args].join(' ') };
}

function probe(command, args, options) {
  const call = invocation(command, args);
  const startedAt = Date.now();
  const result = spawnSync(call.bin, call.args, {
    cwd: repoRoot,
    encoding: 'utf8',
    env: command === 'node' || command === 'npm' ? cleanChildEnv() : childEnvForCommand(command),
    timeout: options.timeoutMs,
    maxBuffer: 512 * 1024,
    windowsHide: true,
  });
  const timedOut = result.error?.code === 'ETIMEDOUT';
  return {
    command: call.display,
    found: result.status === 0 && !result.error && !timedOut,
    code: result.status,
    signal: result.signal ?? null,
    durationMs: Date.now() - startedAt,
    versionText: firstLine(result.stdout) || firstLine(result.stderr),
    stdout: firstLine(result.stdout),
    stderr: firstLine(result.stderr),
    timedOut,
    error: result.error ? String(result.error.message || result.error) : null,
  };
}

function minimumFromEngine(engine, fallback) {
  return String(engine || '').match(/(\d+\.\d+\.\d+)/)?.[1] || fallback;
}

function rustToolchain() {
  try {
    return readText('rust-toolchain.toml').match(/channel\s*=\s*"([^"]+)"/)?.[1] || 'stable';
  } catch {
    return 'stable';
  }
}

function tool(id, purpose, requirement, result, opts = {}) {
  const min = opts.minimumVersion || null;
  const versionOk = !min || versionAtLeast(result.versionText, min);
  let status = 'pass';
  if (!result.found || !versionOk) status = requirement === 'required' ? 'blocked' : 'warn';
  return {
    id,
    purpose,
    requirement,
    status,
    evidence: !result.found ? (result.error || result.stderr || 'not found') : !versionOk ? `${result.versionText || 'unknown'}; expected >= ${min}` : (result.versionText || 'found'),
    minimumVersion: min,
    installHint: opts.installHint || null,
    probe: result,
  };
}

function buildReport(options = {}) {
  const pkg = readJson('package.json');
  const nodeMin = minimumFromEngine(pkg.engines?.node, '22.0.0');
  const npmMin = minimumFromEngine(pkg.engines?.npm, '10.0.0');
  const rust = rustToolchain();
  const checks = [
    tool('node', 'Node automation, npm launcher tests, package harnesses', 'required', probe('node', ['--version'], options), { minimumVersion: nodeMin, installHint: `Install Node >= ${nodeMin}.` }),
    tool('npm', 'npm scripts, pack dry-runs, platform package validation', 'required', probe('npm', ['--version'], options), { minimumVersion: npmMin, installHint: `Install npm >= ${npmMin}.` }),
    tool('cargo', 'Rust build/test/release proof', 'required', probe('cargo', ['--version'], options), { installHint: `Install Rust toolchain ${rust}.` }),
    tool('rustc', 'Native Rust compiler for MCPace binary', 'required', probe('rustc', ['--version'], options), { installHint: `Install Rust toolchain ${rust}.` }),
    tool('rustfmt', 'Rust formatting gate', 'required', probe('cargo', ['fmt', '--version'], options), { installHint: 'rustup component add rustfmt' }),
    tool('clippy', 'Rust lint gate', 'required', probe('cargo', ['clippy', '--version'], options), { installHint: 'rustup component add clippy' }),
    tool('git', 'Patch generation, local hooks, release diffs', 'recommended', probe('git', ['--version'], options), { installHint: 'Install git for patch/release discipline.' }),
    tool('cargo-nextest', 'Faster local Rust test runner and flaky diagnostics', 'recommended', probe('cargo', ['nextest', '--version'], options), { installHint: 'Install cargo-nextest for fast local Rust test loops.' }),
    tool('cargo-audit', 'RustSec advisory scan for Cargo.lock', 'recommended', probe('cargo', ['audit', '--version'], options), { installHint: 'Install cargo-audit before public release.' }),
    tool('cargo-deny', 'Dependency license/source/advisory/bans policy', 'recommended', probe('cargo', ['deny', '--version'], options), { installHint: 'Install cargo-deny before public release.' }),
    tool('cargo-auditable', 'Embeds dependency metadata into release binaries', 'optional', probe('cargo', ['auditable', '--version'], options), { installHint: 'Install cargo-auditable for stronger binary auditability.' }),
  ];
  const blocked = checks.filter((c) => c.status === 'blocked');
  const warnings = checks.filter((c) => c.status === 'warn');
  return {
    schema: 'mcpace.toolingReadiness.v1',
    generatedAt: new Date().toISOString(),
    project: { name: deriveProjectName(), version: deriveProjectVersion() },
    status: blocked.length ? 'blocked' : warnings.length ? 'ready-with-warnings' : 'ready',
    localOnly: true,
    githubPaidPlanRequired: false,
    rustToolchain: rust,
    summary: { total: checks.length, pass: checks.filter((c) => c.status === 'pass').length, warn: warnings.length, blocked: blocked.length },
    tools: checks,
    nextActions: [...blocked, ...warnings].map((c) => c.installHint).filter(Boolean),
  };
}

function renderMarkdown(report) {
  const rows = report.tools.map((t) => `| ${t.id} | ${t.requirement} | ${t.status} | ${String(t.evidence).replace(/\|/g, '\\|')} |`);
  return [
    '# MCPace local tooling readiness', '',
    `Project: \`${report.project.name}\` v\`${report.project.version}\``,
    `Status: \`${report.status}\``,
    `GitHub paid plan required: \`${report.githubPaidPlanRequired ? 'yes' : 'no'}\``,
    '', '## Tools', '', '| tool | requirement | status | evidence |', '|---|---:|---:|---|', ...rows,
    report.nextActions.length ? '\n## Next actions\n' : '',
    ...report.nextActions.map((a) => `- ${a}`), '',
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
    process.stdout.write('Usage: node scripts/tooling-readiness.mjs [--json] [--write <path>] [--markdown <path>] [--strict]\n');
    return;
  }
  const report = buildReport(opts);
  writeArtifacts(report, opts);
  process.stdout.write(opts.json ? `${JSON.stringify(report, null, 2)}\n` : renderMarkdown(report));
  if (opts.strict && report.status === 'blocked') process.exitCode = 1;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { main(); } catch (err) { process.stderr.write(`${err.message || err}\n`); process.exitCode = 1; }
}

export { buildReport, renderMarkdown };
