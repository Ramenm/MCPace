#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { deriveProjectName, deriveProjectVersion, repoRoot } from './lib/project-metadata.mjs';
import { childEnvForCommand } from './lib/safe-child-env.mjs';

const DEFAULT_TIMEOUT_MS = 180_000;
const DEFAULT_MAX_BUFFER_BYTES = 16 * 1024 * 1024;

function parseArgs(argv) {
  const out = { profile: 'source', json: false, write: null, markdown: null, planOnly: false, timeoutMs: DEFAULT_TIMEOUT_MS, maxBufferBytes: DEFAULT_MAX_BUFFER_BYTES, strict: false, help: false };
  for (let i = 0; i < argv.length; i += 1) {
    const a = argv[i];
    if (a === '--profile') out.profile = argv[++i] || 'source';
    else if (a === '--json') out.json = true;
    else if (a === '--write') out.write = argv[++i] || null;
    else if (a === '--markdown' || a === '--write-md') out.markdown = argv[++i] || null;
    else if (a === '--plan-only') out.planOnly = true;
    else if (a === '--strict') out.strict = true;
    else if (a === '--timeout-ms') out.timeoutMs = positiveInt(argv[++i], '--timeout-ms');
    else if (a === '--max-buffer-bytes') out.maxBufferBytes = positiveInt(argv[++i], '--max-buffer-bytes');
    else if (a === '-h' || a === '--help') out.help = true;
    else throw new Error(`unsupported local-quality-suite argument: ${a}`);
  }
  if (!['smoke', 'source', 'full', 'release'].includes(out.profile)) throw new Error('--profile must be smoke, source, full, or release');
  return out;
}

function positiveInt(value, flag) {
  if (!/^\d+$/.test(String(value || ''))) throw new Error(`${flag} must be a positive integer`);
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) throw new Error(`${flag} must be a positive integer`);
  return parsed;
}

function node(id, args, opts = {}) {
  return { id, command: process.execPath, args, required: opts.required ?? true, group: opts.group || 'source', mode: opts.mode || 'json-status', purpose: opts.purpose || id, timeoutMs: opts.timeoutMs || null };
}

function cargo(id, args, opts = {}) {
  return { id, command: 'cargo', args, required: opts.required ?? true, group: opts.group || 'rust', mode: 'exit-code', purpose: opts.purpose || id, timeoutMs: opts.timeoutMs || null };
}

function shellDisplay(step) {
  const command = step.command === process.execPath ? 'node' : step.command;
  return [command, ...step.args].join(' ');
}

function planFor(profile) {
  const smoke = [
    node('node-syntax', ['scripts/check-node-syntax.mjs', '--json', '--write', 'reports/node-syntax-latest.json'], { group: 'source', mode: 'node-syntax', purpose: 'JS/MJS files parse cleanly.' }),
    node('source-audit', ['scripts/audit-source.mjs', '--json', '--fail-on-critical', '--write', 'reports/source-audit-latest.json'], { group: 'source', mode: 'json-status', purpose: 'Critical source-risk patterns are absent.' }),
    node('system-lifecycle-audit', ['scripts/system-lifecycle-audit.mjs', '--json', '--write', 'reports/system-lifecycle-latest.json', '--markdown', 'reports/system-lifecycle-latest.md', '--strict'], { group: 'lifecycle', mode: 'json-status', purpose: 'Install/runtime/restart/reinstall/uninstall lifecycle contract remains consistent.' }),
    node('mixed-upstream-topology', ['scripts/simulate-mixed-upstreams.mjs', '--servers', '50', '--tools', '200000', '--json', '--write', 'reports/mixed-upstreams-latest.json', '--memory-limit-mib', '512'], { group: 'runtime-scale', mode: 'json-status', purpose: 'Mixed stdio/HTTP/blocked/failing upstream topology remains bounded and isolated.' }),
    node('upstream-failsafe', ['scripts/simulate-upstream-failsafe.mjs', '--servers', '50', '--tools', '200000', '--json', '--write', 'reports/upstream-failsafe-latest.json', '--memory-limit-mib', '512'], { group: 'runtime-scale', mode: 'json-status', purpose: 'Server/tool failure scenarios degrade safely with stale-cache semantics, circuit breakers, retries, and batch failure isolation.' }),
    node('tool-exposure-safety', ['scripts/tool-exposure-safety-audit.mjs', '--json', '--write', 'reports/tool-exposure-safety-latest.json', '--strict'], { group: 'runtime-safety', mode: 'json-status', purpose: 'Tool exposure/projection/call routing fails closed for unknown or suspicious tools.' }),
    node('tool-message-integrity', ['scripts/tool-message-integrity-audit.mjs', '--json', '--write', 'reports/tool-message-integrity-latest.json', '--strict'], { group: 'runtime-safety', mode: 'json-status', purpose: 'MCP/JSON-RPC envelope and tool argument shape validation fail closed before dispatch.' }),
    node('defect-gates', ['scripts/defect-gates.mjs', '--json', '--write', 'reports/defect-gates-latest.json', '--markdown', 'reports/defect-gates-latest.md'], { group: 'quality', purpose: 'Bug intake/fix discipline stays enforceable.' }),
    node('bug-sweep', ['scripts/bug-sweep.mjs', '--json', '--write', 'reports/bug-sweep-latest.json', '--markdown', 'reports/bug-sweep-latest.md'], { group: 'quality', mode: 'bug-sweep', purpose: 'Fast sweep over known bug/security invariants.' }),
    node('secret-scan', ['scripts/secret-scan.mjs', '--json', '--write', 'reports/secret-scan-latest.json', '--markdown', 'reports/secret-scan-latest.md'], { group: 'security', purpose: 'Local high-confidence secret scan.' }),
  ];

  const source = [
    node('toolbox-doctor', ['scripts/toolbox-doctor.mjs', '--json', '--write', 'reports/toolbox-doctor-latest.json', '--markdown', 'reports/toolbox-doctor-latest.md'], { group: 'tooling', required: false, purpose: 'Local tool inventory; warnings do not block source snapshots.' }),
    ...smoke,
    node('supply-chain-risk', ['scripts/supply-chain-risk-audit.mjs', '--json', '--write', 'reports/supply-chain-risk-latest.json', '--markdown', 'reports/supply-chain-risk-latest.md'], { group: 'security', required: false, mode: 'warnings-ok', purpose: 'Dependency/release supply-chain posture; optional tools warn if absent.' }),
    node('github-health', ['scripts/github-health-audit.mjs', '--json', '--write', 'reports/github-health-latest.json', '--markdown', 'reports/github-health-latest.md'], { group: 'public-surface', required: false, purpose: 'Public-repository/community health files.' }),
    node('github-readiness', ['scripts/verify-github-readiness.mjs', '--json', '--write', 'reports/github-readiness-latest.json', '--markdown', 'reports/github-readiness-latest.md'], { group: 'public-surface', required: false, purpose: 'Star-friendly public launch surface.' }),
    node('free-tier-readiness', ['scripts/free-tier-readiness.mjs', '--json', '--write', 'reports/free-tier-readiness-latest.json', '--markdown', 'reports/free-tier-readiness-latest.md'], { group: 'public-surface', purpose: 'No paid GitHub plan is required for core proof.' }),
    node('install-readiness-source', ['scripts/install-readiness-harness.mjs', '--json', '--write', 'reports/install-readiness-latest.json', '--no-npm-pack'], { group: 'package', required: false, purpose: 'Source/install-readiness snapshot without native-binary claim.' }),
    node('repo-node-smoke-tests', ['scripts/run-node-test-files.mjs', '--dir', 'tests/node', '--ext', '.test.js', '--json', '--progress', '--timeout-ms', '60000', '--heartbeat-ms', '10000', '--only', 'local-quality-contract', '--only', 'publish-decision-contract', '--only', 'source-quality-contract', '--only', 'security-contract', '--only', 'github-readiness-contract', '--only', 'bug-sweep-contract', '--only', 'defect-gates-contract', '--write', 'reports/node-tests-smoke-latest.json'], { group: 'tests', purpose: 'Fast source-level Node contract tests.' }),
    node('npm-cli-tests', ['scripts/run-node-test-files.mjs', '--dir', 'packages/npm/cli/test', '--ext', '.test.mjs', '--json', '--progress', '--timeout-ms', '90000', '--batch-size', '8', '--write', 'reports/npm-cli-tests-latest.json'], { group: 'tests', purpose: 'npm thin-launcher tests.' }),
    cargo('cargo-metadata', ['metadata', '--no-deps', '--format-version', '1'], { group: 'rust', purpose: 'Cargo manifests parse without dependency download.' }),
    cargo('cargo-fmt', ['fmt', '--all', '--', '--check'], { group: 'rust', purpose: 'Rust formatting proof.' }),
    node('npm-pack', ['scripts/verify-npm-pack.mjs', '--json'], { group: 'package', purpose: 'Thin npm launcher pack dry-run.' }),
    node('platform-package-manifests', ['scripts/verify-platform-packages.mjs', '--json'], { group: 'package', purpose: 'Platform package manifests are aligned.' }),
    node('product-practice', ['scripts/product-practice-harness.mjs', '--json', '--write', 'reports/product-practice-latest.json', '--markdown', 'reports/product-practice-latest.md'], { group: 'claims', required: false, mode: 'warnings-ok', purpose: 'Honest product-claim gate; may block runtime claims while source remains OK.' }),
  ];

  const full = [
    ...source,
    node('rust-quality-full', ['scripts/verify-rust-quality.mjs', '--json', '--write', 'reports/rust-quality-latest.json'], { group: 'rust', purpose: 'Full Rust proof: fmt, clippy, Rust tests, release build.', timeoutMs: 600_000 }),
    node('vendored-binary', ['scripts/verify-vendored-binary.mjs', '--json'], { group: 'package', purpose: 'Host-compatible native binary staged in npm vendor layout.' }),
    node('runtime-trace', ['scripts/runtime-trace-harness.mjs', '--json', '--write', 'reports/runtime-trace-latest.json', '--markdown', 'reports/runtime-trace-latest.md'], { group: 'runtime', purpose: 'client -> /mcp -> initialize -> tools/list -> tools/call proof.' }),
  ];

  const release = [
    ...full,
    node('local-prepublish', ['scripts/local-prepublish-gate.mjs', '--json', '--write', 'reports/local-prepublish-latest.json', '--markdown', 'reports/local-prepublish-latest.md'], { group: 'release', purpose: 'Existing strict local pre-publish gate.' }),
    node('publish-decision', ['scripts/publish-decision.mjs', '--json', '--write', 'reports/publish-decision-latest.json', '--markdown', 'reports/publish-decision-latest.md'], { group: 'release', mode: 'publish-decision', purpose: 'Final source-vs-native-publication decision.' }),
  ];

  return profile === 'smoke' ? smoke : profile === 'source' ? source : profile === 'full' ? full : release;
}

function summarize(text) {
  const lines = String(text || '').split(/\r?\n/).filter(Boolean);
  if (lines.length <= 40) return lines.join('\n');
  return ['…', ...lines.slice(-40)].join('\n');
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

function interpret(step, report, exitOk) {
  if (!exitOk) return { status: step.required ? 'blocked' : 'warn', evidence: 'command failed' };
  if (!report || typeof report !== 'object') return { status: 'pass', evidence: 'exit 0' };
  if (step.mode === 'node-syntax') return { status: report.status === 'pass' ? 'pass' : 'blocked', evidence: `${report.checkedCount ?? report.fileCount ?? '?'} checked` };
  if (step.mode === 'bug-sweep') {
    const blocked = report.summary?.blocked ?? report.summary?.blockers ?? 0;
    const warnings = report.summary?.warnings ?? 0;
    if (blocked > 0 || /blocked|fail/.test(String(report.status || ''))) return { status: step.required ? 'blocked' : 'warn', evidence: `${blocked} blockers` };
    return { status: warnings > 0 ? 'warn' : 'pass', evidence: warnings > 0 ? `${warnings} warnings` : String(report.status || 'pass') };
  }
  if (step.mode === 'publish-decision') return { status: report.okForPublicSourceSnapshot ? (report.okForNpmNativePublication ? 'pass' : 'warn') : 'blocked', evidence: report.status || 'unknown' };
  if (step.mode === 'warnings-ok') return { status: /blocked|fail/.test(String(report.status || '')) && step.required ? 'blocked' : /warn|blocked|prove|stage/.test(String(report.status || '')) ? 'warn' : 'pass', evidence: report.status || 'ok' };
  const s = String(report.status ?? (report.ok === true ? 'pass' : 'pass'));
  if (/blocked|fail|timeout/.test(s)) return { status: step.required ? 'blocked' : 'warn', evidence: s };
  if (/warn|warning|partial/.test(s)) return { status: 'warn', evidence: s };
  return { status: 'pass', evidence: s };
}

function runStep(step, opts) {
  const command = shellDisplay(step);
  if (opts.planOnly) return { id: step.id, group: step.group, required: step.required, status: 'planned', command, purpose: step.purpose };
  const startedAt = Date.now();
  const bin = step.command;
  const args = step.args;
  const envCommand = step.command === process.execPath ? 'node' : step.command;
  const result = spawnSync(bin, args, {
    cwd: repoRoot,
    env: childEnvForCommand(envCommand),
    encoding: 'utf8',
    timeout: step.timeoutMs || opts.timeoutMs,
    maxBuffer: opts.maxBufferBytes,
    windowsHide: true,
  });
  const timedOut = result.error?.code === 'ETIMEDOUT';
  const exitOk = result.status === 0 && !result.error && !timedOut;
  const parsed = parseJson(result.stdout);
  const semantic = interpret(step, parsed, exitOk);
  return {
    id: step.id,
    group: step.group,
    required: step.required,
    status: semantic.status,
    ok: semantic.status !== 'blocked',
    command,
    purpose: step.purpose,
    evidence: semantic.evidence,
    code: result.status,
    signal: result.signal ?? null,
    durationMs: Date.now() - startedAt,
    timeoutMs: step.timeoutMs || opts.timeoutMs,
    stdoutSummary: summarize(result.stdout),
    stderrSummary: summarize(result.stderr),
    error: result.error ? String(result.error.message || result.error) : null,
  };
}

function buildReport(opts) {
  const steps = planFor(opts.profile).map((step) => runStep(step, opts));
  const blockers = steps.filter((step) => step.required && step.status === 'blocked');
  const warnings = steps.filter((step) => step.status === 'warn' || (!step.required && step.status === 'blocked'));
  const status = opts.planOnly ? 'planned' : blockers.length ? 'blocked' : warnings.length ? 'pass-with-warnings' : 'pass';
  return {
    schema: 'mcpace.localQualitySuite.v1',
    generatedAt: new Date().toISOString(),
    project: { name: deriveProjectName(), version: deriveProjectVersion() },
    profile: opts.profile,
    status,
    localOnly: true,
    githubPaidPlanRequired: false,
    summary: { total: steps.length, passed: steps.filter((s) => s.status === 'pass').length, warnings: warnings.length, blockers: blockers.length, planned: steps.filter((s) => s.status === 'planned').length },
    steps,
    decision: opts.profile === 'source' && !blockers.length ? 'Public source snapshot is allowed; native/npm runtime publication still needs full/release proof.' : status === 'pass' ? 'This profile is green.' : status === 'pass-with-warnings' ? 'No required blockers, but warnings remain.' : status === 'planned' ? 'Plan only; no commands ran.' : 'Do not publish or strengthen claims until blockers are fixed.',
    nextActions: [...blockers, ...warnings].map((step) => `${step.id}: ${step.evidence}`),
  };
}

function renderMarkdown(report) {
  return [
    '# MCPace local quality suite', '',
    `Project: \`${report.project.name}\` v\`${report.project.version}\``,
    `Profile: \`${report.profile}\``,
    `Status: \`${report.status}\``,
    `GitHub paid plan required: \`${report.githubPaidPlanRequired ? 'yes' : 'no'}\``,
    '', '## Decision', '', report.decision,
    '', '## Steps', '', '| group | step | required | status | evidence |', '|---|---|---:|---:|---|',
    ...report.steps.map((step) => `| ${step.group} | ${step.id} | ${step.required ? 'yes' : 'no'} | ${step.status} | ${String(step.evidence || '').replace(/\|/g, '\\|')} |`),
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
    process.stdout.write('Usage: node scripts/local-quality-suite.mjs --profile smoke|source|full|release [--json] [--write <path>] [--markdown <path>] [--plan-only]\n');
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

export { buildReport, renderMarkdown, planFor };
