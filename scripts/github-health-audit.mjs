#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { deriveProjectName, deriveProjectVersion, repoRoot } from './lib/project-metadata.mjs';

function parseArgs(argv) {
  const parsed = { json: false, write: null, markdown: null, strict: false, help: false };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--json': parsed.json = true; break;
      case '--write': parsed.write = argv[++index] || null; if (!parsed.write) throw new Error('github-health-audit requires a path after --write'); break;
      case '--markdown':
      case '--write-md': parsed.markdown = argv[++index] || null; if (!parsed.markdown) throw new Error('github-health-audit requires a path after --markdown'); break;
      case '--strict': parsed.strict = true; break;
      case '-h':
      case '--help': parsed.help = true; break;
      default: throw new Error(`unsupported github-health-audit argument: ${token}`);
    }
  }
  return parsed;
}

function printHelp() {
  process.stdout.write('Usage: node scripts/github-health-audit.mjs [--json] [--write <path>] [--markdown <path>] [--strict]\n\nChecks public GitHub launch readiness files, workflows, and proof-gate wiring.\n');
}

function exists(relativePath) {
  return fs.existsSync(path.join(repoRoot, relativePath));
}

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

function check(id, category, ok, evidence, nextAction) {
  return {
    id,
    category,
    status: ok ? 'pass' : 'blocked',
    evidence,
    nextAction: ok ? 'Keep this health gate maintained as the project changes.' : nextAction,
  };
}

function requireFile(relativePath, category, nextAction = `Create ${relativePath}.`) {
  return check(`file:${relativePath}`, category, exists(relativePath), exists(relativePath) ? 'present' : 'missing', nextAction);
}

function textCheck(id, category, relativePath, pattern, evidence, nextAction) {
  if (!exists(relativePath)) return check(id, category, false, `${relativePath} missing`, nextAction || `Create ${relativePath}.`);
  const source = read(relativePath);
  return check(id, category, pattern.test(source), evidence, nextAction || `Update ${relativePath}.`);
}

function countIssueTemplates() {
  const dir = path.join(repoRoot, '.github', 'ISSUE_TEMPLATE');
  if (!fs.existsSync(dir)) return 0;
  return fs.readdirSync(dir).filter((name) => /\.(ya?ml|md)$/i.test(name) && name !== 'config.yml').length;
}

function buildReport() {
  const checks = [
    requireFile('README.md', 'community'),
    requireFile('LICENSE', 'community'),
    requireFile('CONTRIBUTING.md', 'community'),
    requireFile('SECURITY.md', 'community'),
    requireFile('SUPPORT.md', 'community'),
    requireFile('CODE_OF_CONDUCT.md', 'community'),
    requireFile('CHANGELOG.md', 'community'),
    requireFile('ROADMAP.md', 'community'),
    requireFile('CODEOWNERS', 'community'),
    requireFile('.github/pull_request_template.md', 'community'),
    requireFile('.github/dependabot.yml', 'security'),
    requireFile('.github/release.yml', 'release'),
    requireFile('.github/workflows/ci.yml', 'automation'),
    requireFile('.github/workflows/release.yml', 'automation'),
    requireFile('.github/workflows/publish-npm.yml', 'automation'),
    requireFile('docs/github-launch-playbook.md', 'docs'),
    requireFile('docs/product-truth-and-beta-gate.md', 'docs'),
    requireFile('docs/release-automation.md', 'docs'),
    textCheck('readme:first-working-path', 'docs', 'README.md', /First working path/i, 'README exposes a fast first path.'),
    textCheck('readme:byo-upstream', 'docs', 'README.md', /Bring Your Own MCP servers|BYO MCP/i, 'README states BYO upstream model.'),
    textCheck('readme:honest-http-upstream', 'docs', 'README.md', /HTTP\/Streamable HTTP entries are inventoried.*fan-out remains blocked/is, 'README does not overclaim HTTP upstream fan-out.'),
    textCheck('security:private-reporting', 'security', 'SECURITY.md', /privately|private vulnerability/i, 'SECURITY.md directs private reporting.'),
    textCheck('support:redaction', 'security', 'SUPPORT.md', /redact|tokens|API keys/i, 'SUPPORT.md tells users to redact secrets.'),
    textCheck('release:trusted-publishing', 'release', '.github/workflows/publish-npm.yml', /id-token:\s*write[\s\S]*npm-publish|npm-publish[\s\S]*id-token:\s*write/i, 'publish workflow is OIDC/trusted-publishing shaped.'),
    textCheck('release:prebuilt-artifacts', 'release', '.github/workflows/publish-npm.yml', /gh release download[\s\S]*verify-release-checksums/i, 'publish workflow verifies prebuilt release artifacts before npm publish.'),
    textCheck('release:checksums', 'release', '.github/workflows/release.yml', /SHA256SUMS\.txt/i, 'release workflow generates checksum assets.'),
    textCheck('proof:product-practice', 'proof', 'package.json', /verify:product-practice/i, 'package scripts expose product-practice proof gate.'),
    textCheck('proof:runtime-trace', 'proof', 'package.json', /verify:runtime-trace/i, 'package scripts expose runtime trace proof gate.'),
    textCheck('proof:github-health', 'proof', 'package.json', /verify:github-health/i, 'package scripts expose GitHub launch health audit.', 'Add verify:github-health script.'),
  ];

  checks.push(check('issues:template-count', 'community', countIssueTemplates() >= 5, `${countIssueTemplates()} issue templates`, 'Keep bug, feature, compatibility, repair, and cleanup templates.'));

  const blocked = checks.filter((entry) => entry.status !== 'pass');
  const byCategory = checks.reduce((acc, entry) => {
    acc[entry.category] ??= { pass: 0, blocked: 0 };
    acc[entry.category][entry.status === 'pass' ? 'pass' : 'blocked'] += 1;
    return acc;
  }, {});
  const score = Math.round((checks.length - blocked.length) / checks.length * 100);
  return {
    schema: 'mcpace.githubHealth.v1',
    generatedAt: new Date().toISOString(),
    project: { name: deriveProjectName(), version: deriveProjectVersion() },
    status: blocked.length === 0 ? 'pass' : 'blocked',
    score,
    summary: { total: checks.length, pass: checks.length - blocked.length, blocked: blocked.length, byCategory },
    checks,
    nextActions: blocked.map((entry) => entry.nextAction),
  };
}

function renderMarkdown(report) {
  const lines = [
    '# MCPace GitHub health audit',
    '',
    `Project: \`${report.project.name}\` v\`${report.project.version}\``,
    `Status: \`${report.status}\``,
    `Score: \`${report.score}%\``,
    '',
    '## Checks',
    '',
    '| check | category | status | evidence |',
    '|---|---|---:|---|',
  ];
  for (const entry of report.checks) {
    lines.push(`| ${entry.id} | ${entry.category} | ${entry.status} | ${String(entry.evidence).replace(/\|/g, '\\|')} |`);
  }
  if (report.nextActions.length > 0) {
    lines.push('', '## Next actions', '');
    for (const action of report.nextActions) lines.push(`- ${action}`);
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
    else process.stdout.write(`[mcpace github-health] ${report.status} ${report.score}%\n`);
    if (parsed.strict && report.status !== 'pass') process.exit(1);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exit(1);
  }
}

if (isCliInvocation()) main();
