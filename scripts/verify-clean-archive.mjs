#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { repoRoot as defaultRepoRoot } from './lib/project-metadata.mjs';
import { listWorkingTreeFiles } from './lib/repo-files.mjs';
import { listZipEntries } from './lib/zip-writer.mjs';
import { normalizeArchivePath, sourceArchivePolicyViolations } from './lib/source-archive-policy.mjs';

function parseArgs(argv) {
  const args = { json: false, repoRoot: defaultRepoRoot, archives: [], sourceTree: false };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--json') args.json = true;
    else if (arg === '--repo') args.repoRoot = path.resolve(argv[++index]);
    else if (arg === '--archive') args.archives.push(path.resolve(argv[++index]));
    else if (arg === '--source-tree') args.sourceTree = true;
    else if (arg === '--help' || arg === '-h') {
      console.log('Usage: node scripts/verify-clean-archive.mjs [--json] [--repo DIR] [--source-tree] [--archive ZIP ...] [DIR|ZIP ...]');
      process.exit(0);
    } else if (!arg.startsWith('-')) {
      const resolved = path.resolve(arg);
      if (fs.existsSync(resolved) && fs.statSync(resolved).isDirectory()) {
        args.repoRoot = resolved;
        args.sourceTree = true;
      } else {
        args.archives.push(resolved);
      }
    } else {
      throw new Error(`unknown argument: ${arg}`);
    }
  }
  return args;
}

function walkFiles(root) {
  return listWorkingTreeFiles(root)
    .map((file) => normalizeArchivePath(path.relative(root, file)))
    .sort();
}

function checkArchive(archivePath) {
  const entries = listZipEntries(archivePath);
  const violations = sourceArchivePolicyViolations(entries);
  return {
    kind: 'archive',
    path: archivePath,
    entries: entries.length,
    status: violations.length === 0 ? 'pass' : 'fail',
    violations,
  };
}

function checkSourceTree(repoRoot) {
  const entries = walkFiles(repoRoot);
  const violations = sourceArchivePolicyViolations(entries, { allowSingleRoot: false });
  return {
    kind: 'source-tree',
    path: '.',
    entries: entries.length,
    status: violations.length === 0 ? 'pass' : 'fail',
    violations,
  };
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const checks = [];
  if (args.sourceTree || args.archives.length === 0) checks.push(checkSourceTree(args.repoRoot));
  for (const archive of args.archives) checks.push(checkArchive(archive));

  const failures = checks.filter((check) => check.status !== 'pass');
  const report = {
    schema: 'mcpace.cleanArchiveVerification.v1',
    status: failures.length === 0 ? 'pass' : 'fail',
    checkedAt: new Date().toISOString(),
    repoRoot: '.',
    failures: failures.length,
    checks,
  };

  if (args.json) console.log(JSON.stringify(report, null, 2));
  else {
    console.log(`${report.status}: ${checks.length} clean archive/source checks, ${failures.length} failures`);
    for (const check of checks) {
      console.log(`- ${check.status}: ${check.kind} ${check.path} (${check.entries} entries)`);
      for (const violation of check.violations.slice(0, 20)) console.log(`  - ${violation.path}: ${violation.reason}`);
      if (check.violations.length > 20) console.log(`  - ... ${check.violations.length - 20} more`);
    }
  }
  process.exitCode = failures.length === 0 ? 0 : 1;
}

try {
  main();
} catch (error) {
  if (process.argv.includes('--json')) {
    console.log(JSON.stringify({ schema: 'mcpace.cleanArchiveVerification.v1', status: 'fail', error: error?.message ?? String(error) }, null, 2));
  } else {
    console.error(error?.stack ?? String(error));
  }
  process.exitCode = 1;
}
