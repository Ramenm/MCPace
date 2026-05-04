#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
import { deriveProjectName, deriveProjectVersion, readJson, repoRoot } from './lib/project-metadata.mjs';

function parseArgs(argv) {
  const out = { json: false, write: null, markdown: null, help: false };
  for (let i = 0; i < argv.length; i += 1) {
    const a = argv[i];
    if (a === '--json') out.json = true;
    else if (a === '--write') out.write = argv[++i] || null;
    else if (a === '--markdown' || a === '--write-md') out.markdown = argv[++i] || null;
    else if (a === '-h' || a === '--help') out.help = true;
    else throw new Error(`unsupported free-tier-readiness argument: ${a}`);
  }
  return out;
}

function exists(relative) { return fs.existsSync(path.join(repoRoot, relative)); }
function text(relative) { return fs.readFileSync(path.join(repoRoot, relative), 'utf8'); }
function pass(id, evidence, nextAction = 'Keep this in place.', severity = 'required') { return { id, status: 'pass', evidence, nextAction, severity }; }
function block(id, evidence, nextAction, severity = 'required') { return { id, status: 'block', evidence, nextAction, severity }; }
function warn(id, evidence, nextAction, severity = 'warning') { return { id, status: 'warn', evidence, nextAction, severity }; }

function buildReport() {
  const pkg = readJson('package.json');
  const scripts = pkg.scripts || {};
  const requiredScripts = ['verify:toolbox', 'verify:local:smoke', 'verify:local:source', 'verify:local:full', 'verify:local:release', 'verify:local:publish', 'verify:secrets', 'verify:supply-chain', 'verify:publish-decision'];
  const missingScripts = requiredScripts.filter((name) => !scripts[name]);
  const gates = [];
  gates.push(missingScripts.length ? block('local-first-package-scripts', `missing scripts: ${missingScripts.join(', ')}`, 'Add/restore the local-first package scripts before relying on GitHub.') : pass('local-first-package-scripts', `${requiredScripts.length} local/free-tier scripts present.`));

  const docFiles = ['docs/offline-quality-and-publish-gates.md', 'docs/local-quality-without-paid-github.md', 'docs/release-decision-runbook.md'];
  const missingDocs = docFiles.filter((file) => !exists(file));
  gates.push(missingDocs.length ? block('local-first-docs', `missing docs: ${missingDocs.join(', ')}`, 'Document the local-first release path.') : pass('local-first-docs', 'Local-first, no-paid-GitHub, and release decision docs are present.'));

  const readme = exists('README.md') ? text('README.md') : '';
  gates.push(/verify:local:source/.test(readme) && /paid GitHub plan/i.test(readme) ? pass('readme-local-first', 'README explains local source proof and paid GitHub is not required.') : warn('readme-local-first', 'README does not clearly explain local source proof/no-paid-GitHub mode.', 'Keep README explicit about local-first proof.'));

  const workflows = exists('.github/workflows') ? fs.readdirSync(path.join(repoRoot, '.github/workflows')).filter((f) => f.endsWith('.yml') || f.endsWith('.yaml')) : [];
  const workflowText = workflows.map((file) => text(path.join('.github/workflows', file))).join('\n');
  gates.push(workflows.length ? pass('github-workflows-optional', `${workflows.length} workflows present; local scripts remain source of truth.`, 'Keep workflows optional mirrors of local scripts.') : warn('github-workflows-optional', 'No workflows present; local path still works.', 'Optional: add public/free GitHub workflow mirrors.'));
  gates.push(/secrets\.NPM_TOKEN/.test(workflowText) ? warn('legacy-npm-token-risk', 'workflow references secrets.NPM_TOKEN', 'Prefer npm trusted publishing/OIDC when publishing from CI; keep local dry-run path independent.') : pass('no-long-lived-npm-token-required', 'No required NPM_TOKEN workflow dependency detected.'));
  gates.push(/id-token:\s*write/.test(workflowText) || /trusted/i.test(workflowText) ? pass('trusted-publishing-shape', 'OIDC/trusted-publishing shape is present or documented.') : warn('trusted-publishing-shape', 'No OIDC trusted-publishing shape detected.', 'Use trusted publishing if npm publication is automated.'));

  const reportFiles = ['reports/local-quality-source-latest.json', 'reports/secret-scan-latest.json', 'reports/supply-chain-risk-latest.json', 'reports/publish-decision-latest.json'];
  const missingReports = reportFiles.filter((file) => !exists(file));
  gates.push(missingReports.length ? warn('local-proof-reports-present', `missing latest reports: ${missingReports.join(', ')}`, 'Run npm run prove:local-first before public source snapshots.') : pass('local-proof-reports-present', `${reportFiles.length} local/free-tier reports present.`));

  const blockers = gates.filter((g) => g.status === 'block');
  const warnings = gates.filter((g) => g.status === 'warn');
  return {
    schema: 'mcpace.freeTierReadiness.v1',
    generatedAt: new Date().toISOString(),
    project: { name: deriveProjectName(), version: deriveProjectVersion() },
    status: blockers.length ? 'blocked' : warnings.length ? 'ready-with-warnings' : 'ready',
    summary: { total: gates.length, passed: gates.filter((g) => g.status === 'pass').length, warnings: warnings.length, blockers: blockers.length },
    gates,
    policy: {
      paidGithubRequired: false,
      localSourceOfTruth: 'Local scripts and generated reports prove source/package/runtime readiness before GitHub mirrors it.',
      publicRepoNote: 'A public repository can use free hosted GitHub Actions/security features, but release decisions should not require a paid plan.',
    },
  };
}

function renderMarkdown(report) {
  return [
    '# MCPace free-tier/local-first readiness', '',
    `Project: \`${report.project.name}\` v\`${report.project.version}\``,
    `Status: \`${report.status}\``,
    `Paid GitHub required: \`${report.policy.paidGithubRequired ? 'yes' : 'no'}\``,
    '', '| gate | status | evidence |', '|---|---:|---|',
    ...report.gates.map((g) => `| ${g.id} | ${g.status} | ${String(g.evidence || '').replace(/\|/g, '\\|')} |`),
    '', '## Policy', '',
    `- ${report.policy.localSourceOfTruth}`,
    `- ${report.policy.publicRepoNote}`,
    '',
  ].join('\n');
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
    process.stdout.write('Usage: node scripts/free-tier-readiness.mjs [--json] [--write <path>] [--markdown <path>]\n');
    return;
  }
  const report = buildReport();
  writeArtifacts(report, opts);
  process.stdout.write(opts.json ? `${JSON.stringify(report, null, 2)}\n` : renderMarkdown(report));
  if (report.summary.blockers > 0) process.exitCode = 1;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { main(); } catch (err) { process.stderr.write(`${err.message || err}\n`); process.exitCode = 1; }
}

export { buildReport, renderMarkdown };
