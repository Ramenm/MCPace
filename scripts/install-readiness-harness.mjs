#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { runBootHarness } from './boot-harness.mjs';

function parseArgs(argv) {
  const parsed = { json: false, write: null, strict: false, skipNpmPack: false, help: false };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--json': parsed.json = true; break;
      case '--write': parsed.write = argv[++index] || 'reports/install-readiness-latest.json'; break;
      case '--strict': parsed.strict = true; break;
      case '--no-npm-pack': parsed.skipNpmPack = true; break;
      case '-h':
      case '--help': parsed.help = true; break;
      default: throw new Error(`unsupported install-readiness argument: ${token}`);
    }
  }
  return parsed;
}

function printHelp() { process.stdout.write('Usage: node scripts/install-readiness-harness.mjs [--json] [--write <path>] [--strict] [--no-npm-pack]\n'); }
function publicStatus(bootStatus) { return bootStatus === 'pass' ? 'ready' : bootStatus === 'partial' ? 'ready-with-warnings' : 'not-ready'; }

export function collectInstallReadiness(options = {}) {
  const boot = runBootHarness(options);
  return {
    schema: 'mcpace.installReadiness.v1',
    generatedAt: new Date().toISOString(),
    project: boot.project,
    status: publicStatus(boot.installReadiness.status),
    bootHarnessStatus: boot.installReadiness.status,
    checks: [
      { id: 'source-inventory', status: boot.inventory.ok ? 'pass' : 'fail', detail: `${boot.inventory.summary.totalFiles} files inventoried` },
      { id: 'source-audit', status: boot.sourceAudit.status, detail: boot.sourceAudit.reason || boot.sourceAudit.output || null },
      { id: 'npm-pack', status: boot.npmPack.status, detail: boot.npmPack.reason || boot.npmPack.packageMode || null },
      { id: 'binary-distribution', status: boot.binaryDistribution.readyForPublishedInstall ? 'pass' : 'warn', detail: boot.binaryDistribution.mode }
    ],
    warnings: boot.installReadiness.warnings,
    blockers: boot.installReadiness.blockers,
    nextCommands: [
      'npm run verify:boot',
      'cargo check --all-targets --locked',
      'cargo test --all-targets --locked',
      'mcpace connect --json',
      'mcpace server test <name> --refresh --json'
    ],
    bootHarness: boot
  };
}

function writeJson(filePath, report) { const target = path.resolve(filePath); fs.mkdirSync(path.dirname(target), { recursive: true }); fs.writeFileSync(target, `${JSON.stringify(report, null, 2)}\n`, 'utf8'); }
function isCliInvocation() { const entry = process.argv[1]; return entry ? pathToFileURL(path.resolve(entry)).href === import.meta.url : false; }
function main() {
  try {
    const parsed = parseArgs(process.argv.slice(2));
    if (parsed.help) { printHelp(); return; }
    const report = collectInstallReadiness(parsed);
    if (parsed.write) writeJson(parsed.write, report);
    if (parsed.json) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
    else process.stdout.write(`${report.status}\n`);
    if (parsed.strict && report.status !== 'ready') process.exit(1);
  } catch (error) { process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`); process.exit(1); }
}
if (isCliInvocation()) main();
