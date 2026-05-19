#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
import { deriveProjectName, deriveProjectVersion, repoRoot } from './lib/project-metadata.mjs';

const DEFAULT_MAX_AGE_HOURS = 6;

function parseArgs(argv) {
  const out = { json: false, write: null, markdown: null, maxAgeHours: DEFAULT_MAX_AGE_HOURS, strictSourceSnapshot: false, strictNativePublication: false, help: false };
  for (let i = 0; i < argv.length; i += 1) {
    const a = argv[i];
    if (a === '--json') out.json = true;
    else if (a === '--write') out.write = argv[++i] || null;
    else if (a === '--markdown' || a === '--write-md') out.markdown = argv[++i] || null;
    else if (a === '--max-age-hours') out.maxAgeHours = Number(argv[++i] || DEFAULT_MAX_AGE_HOURS);
    else if (a === '--strict-source-snapshot') out.strictSourceSnapshot = true;
    else if (a === '--strict-native-publication' || a === '--strict-release') out.strictNativePublication = true;
    else if (a === '-h' || a === '--help') out.help = true;
    else throw new Error(`unsupported publish-decision argument: ${a}`);
  }
  if (!Number.isFinite(out.maxAgeHours) || out.maxAgeHours <= 0) out.maxAgeHours = DEFAULT_MAX_AGE_HOURS;
  return out;
}

function readReport(relative) {
  const absolute = path.join(repoRoot, relative);
  if (!fs.existsSync(absolute)) return { exists: false, relative, status: 'missing', report: null, ageMs: null, fresh: false };
  try {
    const report = JSON.parse(fs.readFileSync(absolute, 'utf8'));
    const generatedAt = report.generatedAt ? Date.parse(report.generatedAt) : NaN;
    const ageMs = Number.isFinite(generatedAt) ? Date.now() - generatedAt : null;
    return { exists: true, relative, status: report.status || 'unknown', generatedAt: report.generatedAt || null, report, ageMs, fresh: true };
  } catch (err) {
    return { exists: true, relative, status: 'unreadable', report: null, error: String(err.message || err), ageMs: null, fresh: false };
  }
}

function isFresh(read, maxAgeHours) {
  return read.exists && read.ageMs !== null && read.ageMs <= maxAgeHours * 60 * 60 * 1000;
}

function freshEvidence(read, maxAgeHours) {
  if (!read.exists) return `${read.relative} missing`;
  if (read.ageMs === null) return `${read.relative} has no parseable generatedAt`;
  const hours = Math.round((read.ageMs / 3600000) * 10) / 10;
  return `${read.relative}: ${read.status}, ${hours}h old, max ${maxAgeHours}h`;
}

function gate(id, scope, requiredFor, ok, status, evidence, nextAction, details = null) {
  return { id, scope, requiredFor, ok, status: ok ? status : 'blocked', evidence, nextAction, details };
}

function reportOk(read, allowedStatuses, opts) {
  return isFresh(read, opts.maxAgeHours) && allowedStatuses.includes(String(read.status));
}

function sourceGates(opts) {
  const localSource = readReport('reports/local-quality-source-latest.json');
  const secretScan = readReport('reports/secret-scan-latest.json');
  const supply = readReport('reports/supply-chain-risk-latest.json');
  const freeTier = readReport('reports/free-tier-readiness-latest.json');
  const product = readReport('reports/product-practice-latest.json');
  return [
    gate('local-quality-source', 'source', 'public source snapshot', reportOk(localSource, ['pass', 'pass-with-warnings'], opts), 'pass', freshEvidence(localSource, opts.maxAgeHours), 'Run npm run verify:local:source.', localSource),
    gate('secret-scan', 'source', 'public source snapshot', reportOk(secretScan, ['pass', 'pass-with-warnings'], opts) && (secretScan.report?.summary?.critical || 0) === 0, 'pass', freshEvidence(secretScan, opts.maxAgeHours), 'Remove/rotate secrets and rerun npm run verify:secrets.', secretScan),
    gate('supply-chain-risk', 'source', 'public source snapshot', reportOk(supply, ['pass', 'pass-with-warnings'], opts) && (supply.report?.summary?.blockers || 0) === 0, supply.status === 'pass-with-warnings' ? 'warning' : 'pass', freshEvidence(supply, opts.maxAgeHours), 'Review supply-chain warnings before a polished launch.', supply),
    gate('free-tier-readiness', 'source', 'public source snapshot', reportOk(freeTier, ['ready', 'ready-with-warnings'], opts) && (freeTier.report?.summary?.blockers || 0) === 0, freeTier.status === 'ready-with-warnings' ? 'warning' : 'pass', freshEvidence(freeTier, opts.maxAgeHours), 'Keep local/free-tier proof path intact.', freeTier),
    gate('product-practice-source', 'source', 'public source snapshot', isFresh(product, opts.maxAgeHours) && product.report?.canClaim?.sourceTreeHealthy === true, 'pass', freshEvidence(product, opts.maxAgeHours), 'Run npm run verify:product-practice and keep claims honest.', product),
  ];
}

function releaseGates(opts) {
  const rust = readReport('reports/rust-quality-latest.json');
  const runtime = readReport('reports/runtime-trace-latest.json');
  const localPublish = readReport('reports/local-prepublish-latest.json');
  const product = readReport('reports/product-practice-latest.json');
  const vendored = readReport('reports/vendored-binary-latest.json');
  const productReleaseOk = isFresh(product, opts.maxAgeHours)
    && product.report?.canClaim?.runtimeBeta === true
    && product.report?.canClaim?.publishedBinaryInstall === true;
  return [
    gate('rust-quality', 'release', 'npm/native publication', reportOk(rust, ['pass'], opts), 'pass', freshEvidence(rust, opts.maxAgeHours), 'Run npm run verify:rust-quality on a host with Cargo dependency access.', rust),
    gate('runtime-trace', 'release', 'npm/native publication', reportOk(runtime, ['pass'], opts), 'pass', freshEvidence(runtime, opts.maxAgeHours), 'Build/stage the native binary, then run npm run verify:runtime-trace.', runtime),
    gate('vendored-binary', 'release', 'npm/native publication', vendored.exists ? reportOk(vendored, ['pass'], opts) : false, 'pass', vendored.exists ? freshEvidence(vendored, opts.maxAgeHours) : 'reports/vendored-binary-latest.json missing', 'Run npm run verify:vendored-binary after staging a native binary.', vendored),
    gate('local-prepublish', 'release', 'npm/native publication', reportOk(localPublish, ['pass'], opts), 'pass', freshEvidence(localPublish, opts.maxAgeHours), 'Run npm run verify:local-prepublish on the release host.', localPublish),
    gate('product-practice-release', 'release', 'npm/native publication', productReleaseOk, 'pass', product.exists ? `runtimeBeta=${product.report?.canClaim?.runtimeBeta === true}, publishedBinaryInstall=${product.report?.canClaim?.publishedBinaryInstall === true}` : 'product-practice report missing', 'Get fresh Rust, vendored binary, and runtime-trace proof before claiming release readiness.', product),
  ];
}

function buildReport(opts) {
  const gates = [...sourceGates(opts), ...releaseGates(opts)];
  const sourceBlockers = gates.filter((g) => g.scope === 'source' && !g.ok);
  const releaseBlockers = gates.filter((g) => g.scope === 'release' && !g.ok);
  const warnings = gates.filter((g) => g.ok && g.status === 'warning');
  const okForPublicSourceSnapshot = sourceBlockers.length === 0;
  const okForNpmNativePublication = okForPublicSourceSnapshot && releaseBlockers.length === 0;
  const status = okForNpmNativePublication ? 'ready-for-native-publication' : okForPublicSourceSnapshot ? 'source-ready-publish-blocked' : 'blocked';
  return {
    schema: 'mcpace.publishDecision.v1',
    generatedAt: new Date().toISOString(),
    project: { name: deriveProjectName(), version: deriveProjectVersion() },
    status,
    okForPublicSourceSnapshot,
    okForNpmNativePublication,
    paidGithubRequired: false,
    maxAgeHours: opts.maxAgeHours,
    summary: { totalGates: gates.length, passed: gates.filter((g) => g.ok).length, warnings: warnings.length, sourceBlockers: sourceBlockers.length, releaseBlockers: releaseBlockers.length },
    gates,
    nextActions: [...sourceBlockers, ...releaseBlockers, ...warnings].map((g) => g.nextAction).filter(Boolean),
  };
}

function renderMarkdown(report) {
  return [
    '# Publish decision', '',
    `Generated: ${report.generatedAt}`,
    `Project: ${report.project.name} ${report.project.version}`,
    `Status: **${report.status}**`, '',
    `Public source snapshot: **${report.okForPublicSourceSnapshot ? 'allowed' : 'blocked'}**`,
    `npm/native publication: **${report.okForNpmNativePublication ? 'allowed' : 'blocked'}**`,
    `Paid GitHub plan required: **${report.paidGithubRequired ? 'yes' : 'no'}**.`,
    '', '| Gate | Scope | Status | Evidence |', '|---|---|---:|---|',
    ...report.gates.map((g) => `| ${g.id} | ${g.scope} | ${g.ok ? g.status : 'blocked'} | ${String(g.evidence || '').replace(/\|/g, '\\|')} |`),
    report.nextActions.length ? '\n## Next actions\n' : '',
    ...[...new Set(report.nextActions)].map((a) => `- ${a}`), '',
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
    process.stdout.write('Usage: node scripts/publish-decision.mjs [--json] [--write <path>] [--markdown <path>] [--max-age-hours <hours>] [--strict-source-snapshot] [--strict-native-publication]\n');
    return;
  }
  const report = buildReport(opts);
  writeArtifacts(report, opts);
  process.stdout.write(opts.json ? `${JSON.stringify(report, null, 2)}\n` : renderMarkdown(report));
  if ((opts.strictSourceSnapshot && !report.okForPublicSourceSnapshot) || (opts.strictNativePublication && !report.okForNpmNativePublication)) {
    process.exitCode = 1;
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { main(); } catch (err) { process.stderr.write(`${err.message || err}\n`); process.exitCode = 1; }
}

export { buildReport, renderMarkdown };
