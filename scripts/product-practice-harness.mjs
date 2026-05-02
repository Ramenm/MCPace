#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { deriveProjectName, deriveProjectVersion, readJson, repoRoot } from './lib/project-metadata.mjs';
import { inventorySource } from './inventory-source.mjs';
import { runSyntaxCheck } from './check-node-syntax.mjs';

function parseArgs(argv) {
  const parsed = { json: false, write: null, markdown: null, strict: false, help: false };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--json': parsed.json = true; break;
      case '--write': parsed.write = argv[++index] || null; if (!parsed.write) throw new Error('product-practice-harness requires a path after --write'); break;
      case '--markdown':
      case '--write-md': parsed.markdown = argv[++index] || null; if (!parsed.markdown) throw new Error('product-practice-harness requires a path after --markdown'); break;
      case '--strict': parsed.strict = true; break;
      case '-h':
      case '--help': parsed.help = true; break;
      default: throw new Error(`unsupported product-practice-harness argument: ${token}`);
    }
  }
  return parsed;
}

function printHelp() {
  process.stdout.write('Usage: node scripts/product-practice-harness.mjs [--json] [--write <path>] [--markdown <path>] [--strict]\n\nChecks whether MCPace is being advanced by the right product proof gates instead of feature/report accumulation.\n');
}

function readJsonIfPresent(relativePath) {
  const fullPath = path.join(repoRoot, relativePath);
  if (!fs.existsSync(fullPath)) return null;
  try { return JSON.parse(fs.readFileSync(fullPath, 'utf8')); } catch { return null; }
}

function reportStatus(relativePath, fallback = 'missing') {
  const report = readJsonIfPresent(relativePath);
  return report?.status || report?.installReadiness?.status || report?.runtimeProof?.status || fallback;
}

function gate(id, status, evidence, nextAction) {
  return { id, status, evidence, nextAction };
}

function buildReport() {
  const inventory = inventorySource({ top: 10 });
  const nodeSyntax = runSyntaxCheck({});
  const rustQuality = readJsonIfPresent('reports/rust-quality-latest.json');
  const boot = readJsonIfPresent('reports/boot-harness-latest.json');
  const runtimeTrace = readJsonIfPresent('reports/runtime-trace-latest.json');
  const installReadiness = readJsonIfPresent('reports/install-readiness-latest.json');
  const pkg = readJson('package.json');
  const lintHardcoded = /node --check .*&&/.test(pkg.scripts['lint:npm'] || '');

  const rustReady = rustQuality?.ok === true || rustQuality?.status === 'pass';
  const runtimeReady = runtimeTrace?.status === 'pass';
  const binaryReady = Boolean(boot?.binaryDistribution?.readyForPublishedInstall || installReadiness?.bootHarness?.binaryDistribution?.readyForPublishedInstall);
  const gates = [
    gate('source-inventory', inventory.ok ? 'pass' : 'blocked', inventory.warnings.join('; ') || 'inventory ok', 'Run npm run inventory:source and fix missing required source assets.'),
    gate('node-syntax', nodeSyntax.status, `${nodeSyntax.checkedCount}/${nodeSyntax.fileCount} JS/MJS files checked`, 'Run npm run lint:npm.'),
    gate('lint-hardcode', lintHardcoded ? 'blocked' : 'pass', pkg.scripts['lint:npm'], 'Keep lint:npm as a small auto-discovery harness, not a hand-maintained file list.'),
    gate('rust-build', rustReady ? 'pass' : 'blocked', reportStatus('reports/rust-quality-latest.json'), 'Run cargo check/test/build on a host with dependency access.'),
    gate('runtime-trace', runtimeReady ? 'pass' : 'blocked', runtimeTrace ? runtimeTrace.status : 'missing reports/runtime-trace-latest.json', 'Capture runtime trace: client -> /mcp -> tools/list -> tools/call -> stdio upstream trace.'),
    gate('published-binary-install', binaryReady ? 'pass' : 'blocked', reportStatus('reports/install-readiness-latest.json'), 'Stage and verify at least one native binary/platform package before claiming published install readiness.'),
  ];
  const wrongPracticeRisks = [];
  if (!rustReady || !runtimeReady) wrongPracticeRisks.push('Feature accumulation can make the project feel done before the actual broker loop is proven.');
  if (!binaryReady) wrongPracticeRisks.push('Thin npm launcher install can be useful, but it is not the same as published native binary install.');
  if (lintHardcoded) wrongPracticeRisks.push('Hand-maintained source file lists create drift; use discovery harnesses.');
  const status = !rustReady ? 'prove-rust-before-runtime-claims' : !runtimeReady ? 'prove-runtime-before-more-features' : !binaryReady ? 'stage-binary-before-publish-claims' : 'ready-for-release-candidate-review';
  return {
    schema: 'mcpace.productPractice.v1',
    generatedAt: new Date().toISOString(),
    project: { name: deriveProjectName(), version: deriveProjectVersion() },
    status,
    canClaim: {
      sourceTreeHealthy: inventory.ok && nodeSyntax.status === 'pass',
      sourceThinLauncherInstall: inventory.ok && nodeSyntax.status === 'pass' && !lintHardcoded,
      runtimeBeta: runtimeReady,
      publishedBinaryInstall: binaryReady,
      universalRemoteMcpBroker: false,
    },
    gates,
    wrongPracticeRisks,
    nextMoves: gates.filter((entry) => entry.status !== 'pass').map((entry) => entry.nextAction),
  };
}

function renderMarkdown(report) {
  const lines = ['# MCPace product-practice harness', '', `Project: \`${report.project.name}\` v\`${report.project.version}\``, `Status: \`${report.status}\``, '', '## Claims', '', '| claim | allowed |', '|---|---:|'];
  for (const [claim, allowed] of Object.entries(report.canClaim)) lines.push(`| ${claim} | ${allowed ? 'yes' : 'no'} |`);
  lines.push('', '## Gates', '', '| gate | status | evidence |', '|---|---:|---|');
  for (const gateEntry of report.gates) lines.push(`| ${gateEntry.id} | ${gateEntry.status} | ${String(gateEntry.evidence).replace(/\|/g, '\\|')} |`);
  if (report.wrongPracticeRisks.length > 0) {
    lines.push('', '## Wrong-practice risks', '');
    for (const risk of report.wrongPracticeRisks) lines.push(`- ${risk}`);
  }
  lines.push('', '## Next moves', '');
  for (const move of report.nextMoves) lines.push(`- ${move}`);
  lines.push('');
  return lines.join('\n');
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
    const report = buildReport();
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
