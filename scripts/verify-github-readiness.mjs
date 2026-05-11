#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { deriveProjectName, deriveProjectVersion, readJson, repoRoot } from './lib/project-metadata.mjs';

function parseArgs(argv) {
  const parsed = { json: false, write: null, markdown: null, strict: false, help: false };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--json': parsed.json = true; break;
      case '--write': parsed.write = argv[++index] || null; if (!parsed.write) throw new Error('verify-github-readiness requires a path after --write'); break;
      case '--markdown':
      case '--write-md': parsed.markdown = argv[++index] || null; if (!parsed.markdown) throw new Error('verify-github-readiness requires a path after --markdown'); break;
      case '--strict': parsed.strict = true; break;
      case '-h':
      case '--help': parsed.help = true; break;
      default: throw new Error(`unsupported verify-github-readiness argument: ${token}`);
    }
  }
  return parsed;
}

function printHelp() {
  process.stdout.write('Usage: node scripts/verify-github-readiness.mjs [--json] [--write <path>] [--markdown <path>] [--strict]\n\nChecks the public GitHub/community/release-facing repository surface without publishing anything.\n');
}

function relative(relativePath) {
  return path.join(repoRoot, relativePath);
}

function readText(relativePath) {
  try { return fs.readFileSync(relative(relativePath), 'utf8'); } catch { return null; }
}

function exists(relativePath) {
  return fs.existsSync(relative(relativePath));
}

function listFiles(relativeDir) {
  const full = relative(relativeDir);
  if (!fs.existsSync(full)) return [];
  return fs.readdirSync(full).filter((entry) => fs.statSync(path.join(full, entry)).isFile()).sort();
}

function includesAll(text, values) {
  return values.every((value) => text.includes(value));
}

function check(id, required, status, evidence, nextAction) {
  return { id, required, status, evidence, nextAction };
}

function pass(id, evidence, nextAction = 'Keep this repo surface maintained as code and docs change.') {
  return check(id, true, 'pass', evidence, nextAction);
}

function warn(id, evidence, nextAction) {
  return check(id, false, 'warn', evidence, nextAction);
}

function block(id, evidence, nextAction) {
  return check(id, true, 'blocked', evidence, nextAction);
}

function requiredFile(relativePath, purpose, requiredText = []) {
  const text = readText(relativePath);
  if (text === null) return block(`file:${relativePath}`, 'missing', `Add ${relativePath} for ${purpose}.`);
  const missing = requiredText.filter((value) => !text.includes(value));
  if (missing.length > 0) {
    return block(`file:${relativePath}`, `missing expected text: ${missing.join(', ')}`, `Update ${relativePath} so it clearly covers ${purpose}.`);
  }
  return pass(`file:${relativePath}`, `${purpose}; ${text.split(/\r?\n/).length} lines`);
}

function optionalFile(relativePath, purpose, requiredText = []) {
  const text = readText(relativePath);
  if (text === null) return warn(`file:${relativePath}`, 'missing', `Add ${relativePath} when this project needs ${purpose}.`);
  const missing = requiredText.filter((value) => !text.includes(value));
  if (missing.length > 0) return warn(`file:${relativePath}`, `missing expected text: ${missing.join(', ')}`, `Update ${relativePath} so it clearly covers ${purpose}.`);
  return check(`file:${relativePath}`, false, 'pass', `${purpose}; ${text.split(/\r?\n/).length} lines`, 'Keep this optional repo surface maintained.');
}

function workflowCheck(relativePath, purpose, requiredText = []) {
  const text = readText(relativePath);
  if (text === null) return block(`workflow:${relativePath}`, 'missing', `Add ${relativePath} for ${purpose}.`);
  const missing = requiredText.filter((value) => !text.includes(value));
  if (missing.length > 0) return block(`workflow:${relativePath}`, `missing expected workflow marker: ${missing.join(', ')}`, `Update ${relativePath} so ${purpose} remains wired.`);
  return pass(`workflow:${relativePath}`, `${purpose}; ${text.split(/\r?\n/).length} lines`);
}

function readPackageJson() {
  try { return readJson('package.json'); } catch { return {}; }
}

function buildReport() {
  const pkg = readPackageJson();
  const issueTemplates = listFiles('.github/ISSUE_TEMPLATE');
  const scripts = pkg.scripts || {};
  const checks = [
    requiredFile('README.md', 'public landing page', ['MCPace', 'First working path', 'Not implemented yet']),
    requiredFile('LICENSE', 'license clarity'),
    requiredFile('CONTRIBUTING.md', 'contribution workflow', ['Supported contributor stack', 'Minimum contributor workflow']),
    requiredFile('SECURITY.md', 'private vulnerability reporting', ['Reporting a vulnerability', 'Current security boundary']),
    requiredFile('CODE_OF_CONDUCT.md', 'community behavior rules', ['Code of Conduct', 'Reporting']),
    requiredFile('SUPPORT.md', 'support boundaries and issue routing', ['Supported channels', 'Before opening an issue']),
    requiredFile('CODEOWNERS', 'review ownership', ['@']),
    requiredFile('.github/pull_request_template.md', 'review discipline', ['Verification', 'Risks', 'Not-tested']),
    requiredFile('.github/dependabot.yml', 'dependency update automation', ['github-actions', 'npm', 'cargo']),
    requiredFile('.github/ISSUE_TEMPLATE/bug_report.yml', 'bug reports with repro and redacted logs', ['Reproduction', 'Logs']),
    requiredFile('.github/ISSUE_TEMPLATE/feature_request.yml', 'feature requests tied to product proof', ['Problem', 'Acceptance criteria']),
    requiredFile('.github/ISSUE_TEMPLATE/runtime-proof.yml', 'community runtime proof submissions', ['Runtime proof', 'Client']),
    requiredFile('ROADMAP.md', 'public roadmap without overclaiming', ['Runtime beta', 'Published install', 'Not the promise yet']),
    requiredFile('docs/github-launch-playbook.md', 'public launch operating plan', ['Launch sequence', 'Stars are earned', 'Repository settings']),
    requiredFile('docs/runtime-beta-roadmap.md', 'runtime beta acceptance criteria', ['Durable HTTP sessions', 'HTTP upstream fan-out', 'Real-client traces']),
    requiredFile('docs/product-truth-and-beta-gate.md', 'truth taxonomy and beta/GA gates', ['Beta gate', 'GA gate']),
    workflowCheck('.github/workflows/ci.yml', 'normal source, Rust, launcher, and hosted proof CI', ['npm test', 'verify:rust-quality']),
    workflowCheck('.github/workflows/release.yml', 'draft GitHub Release and platform artifact proof', ['stage-vendored-binary', 'verify-platform-packages']),
    workflowCheck('.github/workflows/publish-npm.yml', 'npm trusted-publishing lane from release artifacts', ['id-token: write', 'verify-publish-readiness']),
    workflowCheck('.github/workflows/security.yml', 'supply-chain security review lanes', ['dependency-review-action', 'ossf/scorecard-action']),
    workflowCheck('.github/workflows/codeql.yml', 'CodeQL scan for the JavaScript/launcher surface', ['github/codeql-action/init', 'javascript-typescript']),
  ];

  if (!scripts['verify:github-readiness']) {
    checks.push(block('script:verify:github-readiness', 'missing package.json script', 'Add a package script so the launch surface can be checked in CI and locally.'));
  } else {
    checks.push(pass('script:verify:github-readiness', scripts['verify:github-readiness']));
  }

  const readme = readText('README.md') || '';
  const truthMarkers = [
    'HTTP upstream fan-out remains blocked',
    'in-process Streamable HTTP session store',
    'Cross-process persistence and relay-grade auth/session binding remain future work',
    'Keep stronger release claims tied to fresh runtime traces and real-client proof.'
  ];
  const missingTruth = truthMarkers.filter((marker) => !readme.includes(marker));
  if (missingTruth.length > 0) {
    checks.push(block('truthful-readme-claims', `missing truth markers: ${missingTruth.join(', ')}`, 'Keep unimplemented runtime and release claims explicit in README.md.'));
  } else {
    checks.push(pass('truthful-readme-claims', 'README keeps runtime/session/HTTP-upstream limitations explicit.'));
  }

  const expectedTemplates = ['bug_report.yml', 'cleanup-request.yml', 'feature_request.yml', 'repair-report.yml', 'runtime-proof.yml'];
  const missingTemplates = expectedTemplates.filter((template) => !issueTemplates.includes(template));
  if (missingTemplates.length > 0) {
    checks.push(block('issue-template-set', `missing: ${missingTemplates.join(', ')}`, 'Keep bug, feature, repair, cleanup, and runtime proof issue paths available.'));
  } else {
    checks.push(pass('issue-template-set', issueTemplates.join(', ')));
  }

  checks.push(optionalFile('FUNDING.yml', 'funding/sponsorship links once the project is public'));
  checks.push(optionalFile('CITATION.cff', 'academic/software citation metadata if MCPace becomes research-facing'));

  const blockedRequired = checks.filter((entry) => entry.required && entry.status !== 'pass');
  const warnings = checks.filter((entry) => !entry.required && entry.status !== 'pass');
  const status = blockedRequired.length > 0 ? 'blocked' : warnings.length > 0 ? 'ready-with-warnings' : 'pass';
  return {
    schema: 'mcpace.githubReadiness.v1',
    generatedAt: new Date().toISOString(),
    project: { name: deriveProjectName(), version: deriveProjectVersion() },
    status,
    summary: {
      requiredPassed: checks.filter((entry) => entry.required && entry.status === 'pass').length,
      requiredTotal: checks.filter((entry) => entry.required).length,
      warnings: warnings.length,
      issueTemplates,
    },
    checks,
    nextMoves: checks.filter((entry) => entry.status !== 'pass').map((entry) => entry.nextAction),
  };
}

function renderMarkdown(report) {
  const lines = [
    '# MCPace GitHub readiness',
    '',
    `Project: \`${report.project.name}\` v\`${report.project.version}\``,
    `Status: \`${report.status}\``,
    '',
    '## Summary',
    '',
    `- Required checks: \`${report.summary.requiredPassed}/${report.summary.requiredTotal}\``,
    `- Advisory warnings: \`${report.summary.warnings}\``,
    `- Issue templates: \`${report.summary.issueTemplates.join(', ')}\``,
    '',
    '## Checks',
    '',
    '| check | required | status | evidence |',
    '|---|---:|---:|---|',
  ];
  for (const entry of report.checks) {
    lines.push(`| ${entry.id} | ${entry.required ? 'yes' : 'no'} | ${entry.status} | ${String(entry.evidence).replace(/\|/g, '\\|')} |`);
  }
  if (report.nextMoves.length > 0) {
    lines.push('', '## Next moves', '');
    for (const move of report.nextMoves) lines.push(`- ${move}`);
  }
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
    else process.stdout.write(`[mcpace github-readiness] ${report.status}\n`);
    if (parsed.strict && report.status === 'blocked') process.exit(1);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exit(1);
  }
}

if (isCliInvocation()) main();
