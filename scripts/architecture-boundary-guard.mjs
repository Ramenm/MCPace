#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import process from 'node:process';
import { repoRoot as defaultRepoRoot } from './lib/project-metadata.mjs';

const DEFAULT_BUDGETS = Object.freeze({
  inlineTestModules: 0,
  largeRustProductionFiles: 11,
  largeRustTestFiles: 2,
  publicRootModules: 33,
  serviceRsProductionLines: 1200,
});

function parseArgs(argv) {
  const args = { json: false, enforce: false, repoRoot: defaultRepoRoot };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--json') args.json = true;
    else if (arg === '--enforce') args.enforce = true;
    else if (arg === '--repo') args.repoRoot = path.resolve(argv[++index]);
    else if (arg === '--help' || arg === '-h') {
      console.log('Usage: node scripts/architecture-boundary-guard.mjs [--json] [--enforce] [--repo DIR]');
      process.exit(0);
    } else {
      throw new Error(`unknown argument: ${arg}`);
    }
  }
  return args;
}

function normalize(value) {
  return value.split(path.sep).join('/');
}

function readText(file) {
  return fs.existsSync(file) ? fs.readFileSync(file, 'utf8') : '';
}

function lineCount(source) {
  const normalized = source.replace(/\r\n/g, '\n').replace(/\n$/, '');
  return normalized.length === 0 ? 0 : normalized.split('\n').length;
}

function runArchitectureInventory(repoRoot) {
  const result = spawnSync(process.execPath, ['scripts/architecture-debt-inventory.mjs', '--json', '--repo', repoRoot], {
    cwd: repoRoot,
    encoding: 'utf8',
    windowsHide: true,
  });
  if (result.status !== 0) {
    throw new Error(`architecture inventory failed: ${result.stderr || result.stdout}`);
  }
  return JSON.parse(result.stdout);
}

function budgetCheck(id, actual, max, detail) {
  return {
    id,
    ok: actual <= max,
    actual,
    max,
    detail,
  };
}

function booleanCheck(id, ok, detail) {
  return {
    id,
    ok,
    actual: ok,
    expected: true,
    detail,
  };
}

function serviceLegacyQuarantineCheck(repoRoot) {
  const servicePath = path.join(repoRoot, 'src', 'service.rs');
  const legacyPath = path.join(repoRoot, 'src', 'service', 'legacy.rs');
  const service = readText(servicePath);
  const legacy = readText(legacyPath);
  const serviceDefinesLegacyCleanup = /fn\s+(cleanup_legacy_autostart|legacy_autostart_present)\s*\(/.test(service);
  const legacyOwnsCleanup = /fn\s+cleanup_legacy_autostart\s*\(/.test(legacy)
    && /fn\s+legacy_autostart_present\s*\(/.test(legacy);
  return booleanCheck(
    'service-legacy-cleanup-quarantined',
    !serviceDefinesLegacyCleanup && legacyOwnsCleanup,
    'legacy Run-entry cleanup must live in src/service/legacy.rs, while src/service.rs remains the coordinator',
  );
}

function run() {
  const args = parseArgs(process.argv.slice(2));
  const repoRoot = args.repoRoot;
  const inventory = runArchitectureInventory(repoRoot);
  const serviceSource = readText(path.join(repoRoot, 'src', 'service.rs'));
  const serviceLines = lineCount(serviceSource);
  const budgets = DEFAULT_BUDGETS;
  const checks = [
    budgetCheck('inline-test-modules', inventory.summary.inlineTestModules, budgets.inlineTestModules, 'runtime Rust modules should declare external #[cfg(test)] mod tests; files instead of inline test bodies'),
    budgetCheck('large-rust-production-files', inventory.summary.largeRustProductionFiles, budgets.largeRustProductionFiles, 'large production Rust file count should not regress while refactors are phased'),
    budgetCheck('large-rust-test-files', inventory.summary.largeRustTestFiles, budgets.largeRustTestFiles, 'large test file count should not grow while test suites are split'),
    budgetCheck('public-root-modules', inventory.libSurface.publicModuleCount, budgets.publicRootModules, 'src/lib.rs public module surface must not grow while internals are being narrowed'),
    budgetCheck('service-rs-production-lines', serviceLines, budgets.serviceRsProductionLines, 'src/service.rs must stay below the monolith threshold after CLI/config/legacy extraction'),
    serviceLegacyQuarantineCheck(repoRoot),
  ];
  const failures = checks.filter((check) => !check.ok);
  const report = {
    schema: 'mcpace.architectureBoundaryGuard.v1',
    generatedAt: new Date().toISOString(),
    repoRoot: normalize(path.relative(process.cwd(), repoRoot) || '.'),
    status: failures.length === 0 ? 'pass' : 'fail',
    failures: failures.length,
    budgets,
    checks,
  };
  if (args.json) console.log(JSON.stringify(report, null, 2));
  else {
    console.log(`${report.status}: ${checks.length} architecture boundary checks, ${failures.length} failures`);
    for (const check of checks) console.log(`- ${check.ok ? 'pass' : 'fail'} ${check.id}: ${check.actual ?? check.expected}/${check.max ?? check.expected}`);
  }
  if (args.enforce && failures.length > 0) process.exitCode = 1;
}

try {
  run();
} catch (error) {
  console.error(error?.stack ?? String(error));
  process.exitCode = 1;
}
