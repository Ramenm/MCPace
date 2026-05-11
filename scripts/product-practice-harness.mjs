#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { deriveProjectName, deriveProjectVersion, readJson, repoRoot } from './lib/project-metadata.mjs';
import { inventorySource } from './inventory-source.mjs';
import { runSyntaxCheck } from './check-node-syntax.mjs';
import { binaryNameForPlatform, currentTargetKey, detectTarget } from '../packages/npm/cli/lib/platform.js';

const DEFAULT_MAX_PROOF_AGE_MS = 6 * 60 * 60 * 1000;

function parseArgs(argv) {
  const parsed = { json: false, write: null, markdown: null, strict: false, help: false, maxProofAgeMs: maxReportAgeMs() };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--json': parsed.json = true; break;
      case '--write': parsed.write = argv[++index] || null; if (!parsed.write) throw new Error('product-practice-harness requires a path after --write'); break;
      case '--markdown':
      case '--write-md': parsed.markdown = argv[++index] || null; if (!parsed.markdown) throw new Error('product-practice-harness requires a path after --markdown'); break;
      case '--strict': parsed.strict = true; break;
      case '--max-report-age-hours': {
        const hours = Number.parseFloat(argv[++index] || '');
        if (!Number.isFinite(hours) || hours <= 0) throw new Error('product-practice-harness requires a positive number after --max-report-age-hours');
        parsed.maxProofAgeMs = hours * 60 * 60 * 1000;
        break;
      }
      case '-h':
      case '--help': parsed.help = true; break;
      default: throw new Error(`unsupported product-practice-harness argument: ${token}`);
    }
  }
  return parsed;
}

function printHelp() {
  process.stdout.write('Usage: node scripts/product-practice-harness.mjs [--json] [--write <path>] [--markdown <path>] [--strict] [--max-report-age-hours <hours>]\n\nChecks whether MCPace is being advanced by the right product proof gates instead of feature/report accumulation.\n');
}

function maxReportAgeMs() {
  const parsed = Number.parseInt(process.env.MCPACE_MAX_REPORT_AGE_MS || '', 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : DEFAULT_MAX_PROOF_AGE_MS;
}

function generatedAtMs(report) {
  const value = report?.generatedAt || report?.generatedAtIso || report?.generated_at;
  if (!value) return null;
  const parsed = Date.parse(value);
  return Number.isFinite(parsed) ? parsed : null;
}

function ageLabel(ageMs) {
  if (!Number.isFinite(ageMs)) return 'unknown age';
  const minutes = Math.round(ageMs / 60_000);
  if (minutes < 90) return `${minutes}m old`;
  const hours = Math.round(ageMs / 3_600_000);
  if (hours < 48) return `${hours}h old`;
  return `${Math.round(ageMs / 86_400_000)}d old`;
}

function reportFreshness(relativePath, report, now = Date.now(), maxAge = maxReportAgeMs()) {
  if (!report) {
    return { path: relativePath, status: 'missing', fresh: false, generatedAt: null, ageMs: null, maxAgeMs: maxAge, evidence: `missing ${relativePath}` };
  }
  const generated = generatedAtMs(report);
  if (!generated) {
    return { path: relativePath, status: 'unknown', fresh: false, generatedAt: null, ageMs: null, maxAgeMs: maxAge, evidence: `${relativePath} has no generatedAt timestamp` };
  }
  const ageMs = Math.max(0, now - generated);
  const fresh = ageMs <= maxAge;
  return {
    path: relativePath,
    status: fresh ? 'fresh' : 'stale',
    fresh,
    generatedAt: new Date(generated).toISOString(),
    ageMs,
    maxAgeMs: maxAge,
    evidence: `${relativePath} ${fresh ? 'fresh' : 'stale'} (${ageLabel(ageMs)}, max ${ageLabel(maxAge)})`,
  };
}

function readJsonIfPresent(relativePath) {
  const fullPath = path.join(repoRoot, relativePath);
  if (!fs.existsSync(fullPath)) return null;
  try { return JSON.parse(fs.readFileSync(fullPath, 'utf8')); } catch { return null; }
}

function readFirstJsonReport(relativePaths) {
  for (const relativePath of relativePaths) {
    const report = readJsonIfPresent(relativePath);
    if (report) return { relativePath, report };
  }
  return { relativePath: relativePaths[0], report: null };
}

function reportStatus(relativePath, fallback = 'missing') {
  const report = readJsonIfPresent(relativePath);
  return report?.status || report?.installReadiness?.status || report?.runtimeProof?.status || fallback;
}

function currentHostTarget() {
  const target = detectTarget();
  return {
    key: target?.key ?? currentTargetKey(),
    detected: Boolean(target),
    platform: process.platform,
    arch: process.arch,
  };
}

function reportProofStatus(report, expectedVersion, options = {}) {
  const reasons = [];
  if (!report) {
    return { usable: false, reasons: ['missing report'] };
  }
  if (report.project?.version && report.project.version !== expectedVersion) {
    reasons.push(`version mismatch: expected ${expectedVersion}, got ${report.project.version}`);
  }
  if (typeof options.accept === 'function' && !options.accept(report)) {
    reasons.push(options.failureMessage || `report status is ${report.status || report.ok || '<missing>'}`);
  }
  const generatedAtMs = Date.parse(report.generatedAt || '');
  if (!Number.isFinite(generatedAtMs)) {
    reasons.push('missing or invalid generatedAt');
  } else if (Date.now() - generatedAtMs > (options.maxAgeMs ?? DEFAULT_MAX_PROOF_AGE_MS)) {
    const maxHours = ((options.maxAgeMs ?? DEFAULT_MAX_PROOF_AGE_MS) / (60 * 60 * 1000)).toFixed(1);
    const ageHours = ((Date.now() - generatedAtMs) / (60 * 60 * 1000)).toFixed(1);
    reasons.push(`older than ${maxHours}h: ${ageHours}h old`);
  } else if (generatedAtMs - Date.now() > 5 * 60 * 1000) {
    reasons.push('generatedAt is in the future for this host clock');
  }
  return { usable: reasons.length === 0, reasons, maxAgeHours: (options.maxAgeMs ?? DEFAULT_MAX_PROOF_AGE_MS) / (60 * 60 * 1000) };
}

function runtimeTraceProofStatus(report, expectedVersion, options = {}) {
  const base = reportProofStatus(report, expectedVersion, options);
  if (!report) return base;
  const reasons = [...base.reasons];
  if (report.status !== 'pass') {
    reasons.push(`runtime trace status is ${report.status || '<missing>'}`);
  }
  const currentHost = currentHostTarget();
  if (!report.host?.key) {
    reasons.push(`runtime trace lacks host target metadata; rerun npm run verify:runtime-trace on ${currentHost.key}`);
  } else if (report.host.key !== currentHost.key) {
    reasons.push(`runtime trace target is ${report.host.key}, current host target is ${currentHost.key}`);
  }
  const hostBinaryName = binaryNameForPlatform();
  if (report.mode === 'spawned-local-serve') {
    const reportBinary = String(report.binary || '');
    if (!reportBinary.endsWith(hostBinaryName)) {
      reasons.push(`runtime trace binary is not compatible with this host: ${reportBinary || '<missing>'}`);
    }
  } else if (report.mode === 'external-endpoint') {
    const endpoint = String(report.endpoint || '');
    if (!/^https?:\/\/(127\.0\.0\.1|localhost)(:|\/|$)/i.test(endpoint)) {
      reasons.push(`external endpoint is not local: ${endpoint || '<missing>'}`);
    }
  } else {
    reasons.push(`unsupported runtime trace mode: ${report.mode || '<missing>'}`);
  }
  return { usable: reasons.length === 0, reasons };
}

function summarizeProofStatus(proof) {
  return proof.usable ? 'usable' : proof.reasons.join('; ');
}

function gate(id, status, evidence, nextAction) {
  return { id, status, evidence, nextAction };
}

function buildReport(options = {}) {
  const inventory = inventorySource({ top: 10 });
  const nodeSyntax = runSyntaxCheck({});
  const rustQuality = readJsonIfPresent('reports/rust-quality-latest.json');
  const boot = readJsonIfPresent('reports/boot-harness-latest.json');
  const runtimeTrace = readJsonIfPresent('reports/runtime-trace-latest.json');
  const installReadiness = readJsonIfPresent('reports/install-readiness-latest.json');
  const hostTarget = currentHostTarget();
  const vendoredBinaryProofReport = readFirstJsonReport([
    `reports/vendored-binary-${hostTarget.key}.json`,
    'reports/vendored-binary-latest.json',
  ]);
  const vendoredBinary = vendoredBinaryProofReport.report;
  const pkg = readJson('package.json');
  const lintHardcoded = /node --check .*&&/.test(pkg.scripts['lint:npm'] || '');

  const proofOptions = { maxAgeMs: options.maxProofAgeMs ?? DEFAULT_MAX_PROOF_AGE_MS };
  const now = Date.now();
  const freshness = {
    rustQuality: reportFreshness('reports/rust-quality-latest.json', rustQuality, now, proofOptions.maxAgeMs),
    bootHarness: reportFreshness('reports/boot-harness-latest.json', boot, now, proofOptions.maxAgeMs),
    runtimeTrace: reportFreshness('reports/runtime-trace-latest.json', runtimeTrace, now, proofOptions.maxAgeMs),
    installReadiness: reportFreshness('reports/install-readiness-latest.json', installReadiness, now, proofOptions.maxAgeMs),
    vendoredBinary: reportFreshness(vendoredBinaryProofReport.relativePath, vendoredBinary, now, proofOptions.maxAgeMs),
  };
  const rustProof = reportProofStatus(rustQuality, pkg.version, {
    ...proofOptions,
    accept: (report) => report.ok === true || report.status === 'pass',
    failureMessage: `rust quality status is ${rustQuality?.status || rustQuality?.ok || '<missing>'}`,
  });
  const runtimeProof = runtimeTraceProofStatus(runtimeTrace, pkg.version, proofOptions);
  const bootProof = reportProofStatus(boot, pkg.version, proofOptions);
  const installProof = reportProofStatus(installReadiness, pkg.version, proofOptions);
  const vendoredBinaryProof = reportProofStatus(vendoredBinary, pkg.version, {
    ...proofOptions,
    accept: (report) => report.status === 'pass' && report.targetKey === hostTarget.key,
    failureMessage: `vendored binary status is ${vendoredBinary?.status || '<missing>'}; target=${vendoredBinary?.targetKey || '<missing>'}; host=${hostTarget.key}`,
  });
  const rustReady = (rustQuality?.ok === true || rustQuality?.status === 'pass') && rustProof.usable;
  const runtimeReady = runtimeProof.usable;
  const runtimeClaimReady = rustReady && runtimeReady;
  const binaryReady = vendoredBinaryProof.usable;
  const gates = [
    gate('source-inventory', inventory.ok ? 'pass' : 'blocked', inventory.warnings.join('; ') || 'inventory ok', 'Run npm run inventory:source and fix missing required source assets.'),
    gate('node-syntax', nodeSyntax.status, `${nodeSyntax.checkedCount}/${nodeSyntax.fileCount} JS/MJS files checked`, 'Run npm run lint:npm.'),
    gate('lint-hardcode', lintHardcoded ? 'blocked' : 'pass', pkg.scripts['lint:npm'], 'Keep lint:npm as a small auto-discovery harness, not a hand-maintained file list.'),
    gate('rust-build', rustReady ? 'pass' : 'blocked', rustProof.usable ? reportStatus('reports/rust-quality-latest.json') : summarizeProofStatus(rustProof), 'Run cargo check/test/build on a host with dependency access.'),
    gate(
      'runtime-trace',
      runtimeReady ? 'pass' : 'blocked',
      runtimeTrace ? summarizeProofStatus(runtimeProof) : 'missing reports/runtime-trace-latest.json',
      runtimeReady
        ? 'Keep the runtime trace passing before runtime claims.'
        : 'Capture runtime trace: client -> /mcp -> tools/list -> tools/call -> stdio upstream trace.'
    ),
    gate(
      'published-binary-install',
      binaryReady ? 'pass' : 'blocked',
      binaryReady
        ? `${vendoredBinaryProofReport.relativePath}: ${vendoredBinary.status}`
        : `${vendoredBinaryProofReport.relativePath}: ${summarizeProofStatus(vendoredBinaryProof)}`,
      'Run npm run verify:vendored-binary after staging a native binary/platform package before claiming published install readiness.'
    ),
  ];
  const wrongPracticeRisks = [];
  if (!rustReady || !runtimeReady) wrongPracticeRisks.push('Feature accumulation can make the project feel done before the actual broker loop is proven with fresh reports.');
  if (!binaryReady) wrongPracticeRisks.push('Thin npm launcher install can be useful, but it is not the same as published native binary install with fresh artifact proof.');
  if (Object.values(freshness).some((entry) => entry.status === 'stale')) wrongPracticeRisks.push('Stale proof reports can create false confidence; regenerate reports in the same CI or release lane before making runtime/release claims.');
  if (lintHardcoded) wrongPracticeRisks.push('Hand-maintained source file lists create drift; use discovery harnesses.');
  const staleProofs = Object.values(freshness).filter((entry) => entry.status === 'stale');
  const nextMoves = [
    ...gates.filter((entry) => entry.status !== 'pass').map((entry) => entry.nextAction),
    ...staleProofs.map((entry) => `Refresh ${entry.path}: ${entry.evidence}. Regenerate proof reports in the same runtime/release lane before claims.`),
  ];
  const status = !rustReady ? 'prove-rust-before-runtime-claims' : !runtimeReady ? 'prove-runtime-before-more-features' : !binaryReady ? 'stage-binary-before-publish-claims' : 'ready-for-release-candidate-review';
  return {
    schema: 'mcpace.productPractice.v1',
    generatedAt: new Date().toISOString(),
    project: { name: deriveProjectName(), version: deriveProjectVersion() },
    status,
    canClaim: {
      sourceTreeHealthy: inventory.ok && nodeSyntax.status === 'pass',
      sourceThinLauncherInstall: inventory.ok && nodeSyntax.status === 'pass' && !lintHardcoded,
      runtimeBeta: runtimeClaimReady,
      publishedBinaryInstall: binaryReady,
      universalRemoteMcpBroker: false,
    },
    gates,
    freshness,
    wrongPracticeRisks,
    proofValidity: {
      currentHost: hostTarget,
      maxProofAgeHours: proofOptions.maxAgeMs / (60 * 60 * 1000),
      rustQuality: rustProof,
      runtimeTrace: runtimeProof,
      bootHarness: bootProof,
      installReadiness: installProof,
      vendoredBinary: vendoredBinaryProof,
    },
    nextMoves,
  };
}

function renderMarkdown(report) {
  const lines = ['# MCPace product-practice harness', '', `Project: \`${report.project.name}\` v\`${report.project.version}\``, `Status: \`${report.status}\``, '', '## Claims', '', '| claim | allowed |', '|---|---:|'];
  for (const [claim, allowed] of Object.entries(report.canClaim)) lines.push(`| ${claim} | ${allowed ? 'yes' : 'no'} |`);
  lines.push('', '## Proof validity', '');
  lines.push(`Current host: \`${report.proofValidity.currentHost.key}\``);
  lines.push(`Max report age: \`${report.proofValidity.maxProofAgeHours}h\``);
  lines.push('', '## Gates', '', '| gate | status | evidence |', '|---|---:|---|');
  for (const gateEntry of report.gates) lines.push(`| ${gateEntry.id} | ${gateEntry.status} | ${String(gateEntry.evidence).replace(/\|/g, '\\|')} |`);
  if (report.wrongPracticeRisks.length > 0) {
    lines.push('', '## Wrong-practice risks', '');
    for (const risk of report.wrongPracticeRisks) lines.push(`- ${risk}`);
  }
  lines.push('', '## Next moves', '');
  if (report.nextMoves.length === 0) {
    lines.push('- None.');
  } else {
    for (const move of report.nextMoves) lines.push(`- ${move}`);
  }
  return `${lines.join('\n')}\n`;
}

function writeFileEnsuringDir(filePath, contents) {
  const target = path.resolve(repoRoot, filePath);
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, contents, 'utf8');
}

function isCliInvocation() {
  const entry = process.argv[1];
  return entry ? pathToFileURL(path.resolve(entry)).href === import.meta.url : false;
}

function main() {
  try {
    const parsed = parseArgs(process.argv.slice(2));
    if (parsed.help) { printHelp(); return; }
    const report = buildReport({ maxProofAgeMs: parsed.maxProofAgeMs });
    if (parsed.write) writeFileEnsuringDir(parsed.write, `${JSON.stringify(report, null, 2)}\n`);
    if (parsed.markdown) writeFileEnsuringDir(parsed.markdown, renderMarkdown(report));
    if (parsed.json) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
    else process.stdout.write(`[mcpace product-practice] ${report.status}\n`);
    if (parsed.strict && report.status !== 'ready-for-release-candidate-review') process.exit(1);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exit(1);
  }
}

if (isCliInvocation()) main();
