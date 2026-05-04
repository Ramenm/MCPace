#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
import { repoRoot } from './lib/project-metadata.mjs';

function parseArgs(argv) {
  const out = { dryRun: false, force: false, json: false, help: false };
  for (const arg of argv) {
    if (arg === '--dry-run') out.dryRun = true;
    else if (arg === '--force') out.force = true;
    else if (arg === '--json') out.json = true;
    else if (arg === '-h' || arg === '--help') out.help = true;
    else throw new Error(`unsupported install-local-hooks argument: ${arg}`);
  }
  return out;
}

function hookBody() {
  return `#!/usr/bin/env sh
set -eu
printf '%s\n' '[mcpace hooks] running quick local pre-publish gate before push'
npm run verify:local-prepublish:quick
`;
}

function install(opts = {}) {
  const gitDir = path.join(repoRoot, '.git');
  const hooksDir = path.join(gitDir, 'hooks');
  const hookPath = path.join(hooksDir, 'pre-push');
  const existsGit = fs.existsSync(gitDir);
  const report = {
    schema: 'mcpace.localHooksInstall.v1',
    generatedAt: new Date().toISOString(),
    status: 'planned',
    hook: '.git/hooks/pre-push',
    dryRun: Boolean(opts.dryRun),
    force: Boolean(opts.force),
    actions: [],
    blockers: [],
  };
  if (!existsGit) {
    report.status = 'blocked';
    report.blockers.push('No .git directory found; local hooks can only be installed in a checked-out repository.');
    return report;
  }
  if (fs.existsSync(hookPath) && !opts.force) {
    const current = fs.readFileSync(hookPath, 'utf8');
    if (current.includes('verify:local-prepublish:quick')) {
      report.status = 'pass';
      report.actions.push('pre-push hook already installed');
      return report;
    }
    report.status = 'blocked';
    report.blockers.push('pre-push hook already exists; rerun with --force after reviewing it.');
    return report;
  }
  report.actions.push(`write ${path.relative(repoRoot, hookPath).split(path.sep).join('/')}`);
  if (!opts.dryRun) {
    fs.mkdirSync(hooksDir, { recursive: true });
    fs.writeFileSync(hookPath, hookBody(), 'utf8');
    if (process.platform !== 'win32') fs.chmodSync(hookPath, 0o755);
  }
  report.status = opts.dryRun ? 'planned' : 'pass';
  return report;
}

function main() {
  const opts = parseArgs(process.argv.slice(2));
  if (opts.help) {
    process.stdout.write('Usage: node scripts/install-local-hooks.mjs [--dry-run] [--force] [--json]\n');
    return;
  }
  const report = install(opts);
  if (opts.json) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
  else process.stdout.write(`[mcpace hooks] ${report.status}: ${report.actions.concat(report.blockers).join('; ')}\n`);
  if (report.status === 'blocked') process.exitCode = 1;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { main(); } catch (err) { process.stderr.write(`${err.message || err}\n`); process.exitCode = 1; }
}

export { install };
