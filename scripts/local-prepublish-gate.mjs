#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { deriveProjectName, deriveProjectVersion, repoRoot } from './lib/project-metadata.mjs';
import { childEnvForCommand } from './lib/safe-child-env.mjs';

const DEFAULT_TIMEOUT_MS = 300_000;
const DEFAULT_MAX_BUFFER_BYTES = 16 * 1024 * 1024;

function parseArgs(argv) {
  const out = { json: false, write: null, markdown: null, quick: false, planOnly: false, strict: false, timeoutMs: DEFAULT_TIMEOUT_MS, maxBufferBytes: DEFAULT_MAX_BUFFER_BYTES, help: false };
  for (let i = 0; i < argv.length; i += 1) {
    const a = argv[i];
    if (a === '--json') out.json = true;
    else if (a === '--write') out.write = required(argv[++i], '--write');
    else if (a === '--markdown' || a === '--write-md') out.markdown = required(argv[++i], a);
    else if (a === '--quick') out.quick = true;
    else if (a === '--plan-only') out.planOnly = true;
    else if (a === '--strict') out.strict = true;
    else if (a === '--timeout-ms') out.timeoutMs = positiveInt(argv[++i], '--timeout-ms');
    else if (a === '--max-buffer-bytes') out.maxBufferBytes = positiveInt(argv[++i], '--max-buffer-bytes');
    else if (a === '-h' || a === '--help') out.help = true;
    else throw new Error(`unsupported local-prepublish-gate argument: ${a}`);
  }
  return out;
}

function required(value, flag) {
  if (!value) throw new Error(`local-prepublish-gate requires a value after ${flag}`);
  return value;
}

function positiveInt(value, flag) {
  if (!/^\d+$/.test(String(value || ''))) throw new Error(`${flag} must be a positive integer`);
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) throw new Error(`${flag} must be a positive integer`);
  return parsed;
}

function relReport(name, ext = 'json') {
  return `reports/${name}-latest.${ext}`;
}

function nodeStep(id, args, opts = {}) {
  return { id, group: opts.group || 'source', required: opts.required ?? true, command: process.execPath, args, json: opts.json ?? args.includes('--json'), mode: opts.mode || 'status', purpose: opts.purpose || id };
}

function cargoStep(id, args, opts = {}) {
  return { id, group: opts.group || 'rust', required: opts.required ?? true, command: 'cargo', args, json: false, mode: 'exit-code', purpose: opts.purpose || id };
}

function buildPlan(opts = {}) {
  const plan = [
    nodeStep('tooling-readiness', ['scripts/tooling-readiness.mjs', '--json', '--write', relReport('tooling-readiness'), '--markdown', relReport('tooling-readiness', 'md')], { group: 'toolchain', purpose: 'Required local tool versions and recommended release/security tooling.' }),
    nodeStep('node-syntax', ['scripts/check-node-syntax.mjs', '--json', '--write', relReport('node-syntax')], { group: 'source', mode: 'node-syntax', purpose: 'All JS/MJS sources parse.' }),
    nodeStep('source-audit', ['scripts/audit-source.mjs', '--fail-on-critical'], { group: 'source', json: false, mode: 'exit-code', purpose: 'Static source hardening audit.' }),
    nodeStep('defect-gates', ['scripts/defect-gates.mjs', '--json', '--write', relReport('defect-gates'), '--markdown', relReport('defect-gates', 'md')], { group: 'quality', purpose: 'Bug lifecycle, triage, and regression-proof gates.' }),
    nodeStep('bug-sweep', ['scripts/bug-sweep.mjs', '--json', '--write', relReport('bug-sweep'), '--markdown', relReport('bug-sweep', 'md')], { group: 'quality', mode: 'bug-sweep', purpose: 'Fast bug/security invariant sweep.' }),
    nodeStep('public-repo-health', ['scripts/github-health-audit.mjs', '--json', '--write', relReport('github-health'), '--markdown', relReport('github-health', 'md')], { group: 'public-surface', required: false, purpose: 'Repository/community profile health; useful but not dependent on paid GitHub.' }),
    nodeStep('public-readiness', ['scripts/verify-github-readiness.mjs', '--json', '--write', relReport('github-readiness'), '--markdown', relReport('github-readiness', 'md')], { group: 'public-surface', required: false, purpose: 'README/security/contributor/release docs readiness.' }),
    nodeStep('npm-thin-pack', ['scripts/verify-npm-pack.mjs', '--json'], { group: 'package', purpose: 'Thin npm launcher pack verification.' }),
    nodeStep('platform-package-manifests', ['scripts/verify-platform-packages.mjs', '--json'], { group: 'package', purpose: 'Platform package manifest matrix sanity.' }),
    cargoStep('cargo-metadata', ['metadata', '--no-deps', '--format-version', '1'], { group: 'rust', purpose: 'Cargo manifests parse without downloading dependencies.' }),
    cargoStep('cargo-fmt', ['fmt', '--all', '--', '--check'], { group: 'rust', purpose: 'Rust formatting gate.' }),
  ];
  if (!opts.quick) {
    plan.push(
      nodeStep('rust-quality-full', ['scripts/verify-rust-quality.mjs', '--json', '--write', relReport('rust-quality'), '--timeout-ms', String(opts.timeoutMs), '--max-buffer-bytes', String(opts.maxBufferBytes)], { group: 'rust', purpose: 'Full Rust proof: fmt, clippy, Rust tests, release build.' }),
      nodeStep('vendored-binary', ['scripts/verify-vendored-binary.mjs', '--json'], { group: 'package', purpose: 'Host-native binary exists in npm vendor layout.' }),
      nodeStep('runtime-trace', ['scripts/runtime-trace-harness.mjs', '--json', '--write', relReport('runtime-trace'), '--markdown', relReport('runtime-trace', 'md')], { group: 'runtime', purpose: 'client -> /mcp -> initialize -> tools/list -> upstream_call proof.' }),
      nodeStep('install-readiness', ['scripts/install-readiness-harness.mjs', '--json', '--write', relReport('install-readiness')], { group: 'package', purpose: 'Install readiness after binary staging and boot proof.' }),
      nodeStep('product-practice', ['scripts/product-practice-harness.mjs', '--json', '--write', relReport('product-practice'), '--markdown', relReport('product-practice', 'md')], { group: 'release-claim', mode: 'product-practice', purpose: 'Final claim gate; prevents overclaiming runtime/published install readiness.' }),
    );
  }
  return plan;
}

function invocation(step) {
  if (process.platform === 'win32' && step.command === 'cargo') {
    return { bin: 'cmd.exe', args: ['/d', '/s', '/c', 'cargo', ...step.args], display: ['cargo', ...step.args].join(' ') };
  }
  return { bin: step.command, args: step.args, display: [step.command === process.execPath ? 'node' : step.command, ...step.args].join(' ') };
}

function summarize(text) {
  const lines = String(text || '').split(/\r?\n/).filter(Boolean);
  return lines.slice(-20).join('\n');
}

function parseJson(text) {
  const raw = String(text || '').trim();
  if (!raw) return null;
  try { return JSON.parse(raw); } catch { /* fall through */ }
  const start = raw.indexOf('{');
  const end = raw.lastIndexOf('}');
  if (start >= 0 && end > start) {
    try { return JSON.parse(raw.slice(start, end + 1)); } catch { return null; }
  }
  return null;
}

function blockedStatus(status) {
  const s = String(status || '').toLowerCase();
  return s === 'blocked' || s === 'fail' || s === 'failed' || s === 'timeout' || s.includes('prove-rust-before') || s.includes('prove-runtime-before') || s.includes('stage-binary-before');
}

function warningStatus(status) {
  return /warn|warning|partial/.test(String(status || '').toLowerCase());
}

function interpret(step, report) {
  if (!report || typeof report !== 'object') return { status: 'pass', evidence: 'exit 0' };
  if (step.mode === 'node-syntax') return { status: report.status === 'pass' ? 'pass' : 'blocked', evidence: `${report.checkedCount ?? report.passCount ?? '?'} checked` };
  if (step.mode === 'bug-sweep') {
    if ((report.summary?.blocked || 0) > 0 || report.status === 'blocked') return { status: 'blocked', evidence: `${report.summary?.blocked ?? '?'} blockers` };
    if ((report.summary?.warnings || 0) > 0 || warningStatus(report.status)) return { status: 'warn', evidence: `${report.summary?.warnings ?? 0} warnings` };
    return { status: 'pass', evidence: report.status || 'pass' };
  }
  if (step.mode === 'product-practice') return { status: report.status === 'ready-for-release-candidate-review' ? 'pass' : 'blocked', evidence: report.status || 'unknown' };
  const status = report.status ?? (report.ok === true ? 'pass' : null) ?? 'pass';
  if (blockedStatus(status)) return { status: 'blocked', evidence: String(status) };
  if (warningStatus(status)) return { status: 'warn', evidence: String(status) };
  return { status: 'pass', evidence: String(status) };
}

function runStep(step, opts) {
  const call = invocation(step);
  if (opts.planOnly) return { id: step.id, group: step.group, required: step.required, status: 'planned', command: call.display, purpose: step.purpose };
  const startedAt = Date.now();
  const result = spawnSync(call.bin, call.args, {
    cwd: repoRoot,
    env: childEnvForCommand(step.command === process.execPath ? 'node' : step.command),
    encoding: 'utf8',
    timeout: opts.timeoutMs,
    maxBuffer: opts.maxBufferBytes,
    windowsHide: true,
  });
  const timedOut = result.error?.code === 'ETIMEDOUT';
  let status = result.status === 0 && !result.error && !timedOut ? 'pass' : step.required ? 'blocked' : 'warn';
  let evidence = timedOut ? `timed out after ${opts.timeoutMs}ms` : result.error ? String(result.error.message || result.error) : `exit ${result.status}`;
  const report = step.json ? parseJson(result.stdout) : null;
  if (status === 'pass') {
    const semantic = step.json ? interpret(step, report) : { status: 'pass', evidence: 'exit 0' };
    status = semantic.status === 'blocked' && !step.required ? 'warn' : semantic.status;
    evidence = semantic.evidence;
  }
  return { id: step.id, group: step.group, required: step.required, status, command: call.display, purpose: step.purpose, evidence, code: result.status, signal: result.signal ?? null, durationMs: Date.now() - startedAt, timedOut, stdoutSummary: summarize(result.stdout), stderrSummary: summarize(result.stderr), error: result.error ? String(result.error.message || result.error) : null, report };
}

function decision(status, mode) {
  if (status === 'planned') return 'Plan only: no commands were run.';
  if (status === 'pass') return mode === 'quick' ? 'Quick local hygiene is green. Run the full local pre-publish gate before release.' : 'Full local pre-publish gate is green. Release-candidate review can start.';
  if (status === 'pass-with-warnings') return 'No required blockers, but warnings should be resolved before a polished public launch.';
  return 'Do not publish yet: required local proof is missing or failed.';
}

function buildReport(opts = {}) {
  const options = { ...parseArgs([]), ...opts };
  const mode = options.quick ? 'quick' : 'full';
  const steps = buildPlan(options).map((step) => runStep(step, options));
  const blocked = steps.filter((s) => s.required && s.status === 'blocked');
  const warnings = steps.filter((s) => s.status === 'warn');
  const status = options.planOnly ? 'planned' : blocked.length ? 'blocked' : warnings.length ? 'pass-with-warnings' : 'pass';
  return { schema: 'mcpace.localPrepublishGate.v1', generatedAt: new Date().toISOString(), project: { name: deriveProjectName(), version: deriveProjectVersion() }, status, mode, localOnly: true, githubPaidPlanRequired: false, summary: { total: steps.length, pass: steps.filter((s) => s.status === 'pass').length, warn: warnings.length, blocked: blocked.length, planned: steps.filter((s) => s.status === 'planned').length }, steps, decision: decision(status, mode), nextActions: steps.filter((s) => s.status === 'blocked' || s.status === 'warn').map((s) => `${s.id}: ${s.evidence}`) };
}

function renderMarkdown(report) {
  return ['# MCPace local pre-publish gate', '', `Project: \`${report.project.name}\` v\`${report.project.version}\``, `Mode: \`${report.mode}\``, `Status: \`${report.status}\``, `GitHub paid plan required: \`${report.githubPaidPlanRequired ? 'yes' : 'no'}\``, '', '## Decision', '', report.decision, '', '## Steps', '', '| group | step | required | status | evidence |', '|---|---|---:|---:|---|', ...report.steps.map((s) => `| ${s.group} | ${s.id} | ${s.required ? 'yes' : 'no'} | ${s.status} | ${String(s.evidence || '').replace(/\|/g, '\\|')} |`), report.nextActions.length ? '\n## Next actions\n' : '', ...report.nextActions.map((a) => `- ${a}`), ''].filter((line) => line !== '').join('\n');
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
    process.stdout.write('Usage: node scripts/local-prepublish-gate.mjs [--quick] [--plan-only] [--json] [--write <path>] [--markdown <path>] [--strict]\n');
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

export { buildPlan, buildReport, renderMarkdown };
