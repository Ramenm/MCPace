#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';

const FULL_SHA = /^[a-f0-9]{40}$/i;
const BRANCH_LIKE = /^(main|master|trunk|dev|develop|HEAD)$/i;
const SAME_REPO_ACTION_PREFIXES = ['./', '../'];
const UNTRUSTED_INLINE_EXPRESSION = /^(inputs|github\.event|github\.head_ref|github\.ref_name|env|matrix|vars)\b/;

function parseArgs(argv) {
  const args = { json: false, enforceSha: false, repoRoot: process.cwd() };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--json') args.json = true;
    else if (arg === '--enforce-sha') args.enforceSha = true;
    else if (arg === '--repo') args.repoRoot = path.resolve(argv[++index]);
    else if (arg === '--help' || arg === '-h') {
      console.log('Usage: node scripts/verify-workflow-policy.mjs [--json] [--enforce-sha] [--repo DIR]');
      process.exit(0);
    } else {
      throw new Error(`unknown argument: ${arg}`);
    }
  }
  return args;
}

function rel(repoRoot, filePath) {
  return path.relative(repoRoot, filePath).split(path.sep).join('/');
}

function readTextIfExists(filePath) {
  return fs.existsSync(filePath) ? fs.readFileSync(filePath, 'utf8') : '';
}

function workflowFiles(repoRoot) {
  const workflowDir = path.join(repoRoot, '.github/workflows');
  if (!fs.existsSync(workflowDir)) return [];
  return fs.readdirSync(workflowDir)
    .filter((name) => /\.ya?ml$/i.test(name))
    .sort((left, right) => left.localeCompare(right))
    .map((name) => path.join(workflowDir, name));
}

function finding(id, status, detail, extra = {}) {
  return { id, status, detail, ...extra };
}

function extractUsesEntries(repoRoot, filePath, text) {
  const entries = [];
  const file = rel(repoRoot, filePath);
  const lines = text.split(/\r?\n/);
  for (let index = 0; index < lines.length; index += 1) {
    const match = lines[index].match(/^\s*(?:-\s*)?uses:\s*([^\s#]+)\s*(?:#.*)?$/);
    if (!match) continue;
    const value = match[1].replace(/^['"]|['"]$/g, '');
    const local = SAME_REPO_ACTION_PREFIXES.some((prefix) => value.startsWith(prefix));
    const at = value.lastIndexOf('@');
    entries.push({ file, line: index + 1, value, local, action: at >= 0 ? value.slice(0, at) : value, ref: at >= 0 ? value.slice(at + 1) : '' });
  }
  return entries;
}

function extractRunBlocks(repoRoot, filePath, text) {
  const file = rel(repoRoot, filePath);
  const lines = text.split(/\r?\n/);
  const blocks = [];
  for (let index = 0; index < lines.length; index += 1) {
    const header = lines[index].match(/^(\s*)(?:-\s*)?run:\s*\|\s*$/);
    if (!header) continue;
    const indent = header[1].length;
    const body = [];
    let cursor = index + 1;
    while (cursor < lines.length) {
      const line = lines[cursor];
      if (line.trim() === '') {
        body.push(line);
        cursor += 1;
        continue;
      }
      const currentIndent = line.match(/^\s*/)[0].length;
      if (currentIndent <= indent) break;
      body.push(line);
      cursor += 1;
    }
    blocks.push({ file, line: index + 1, body: body.join('\n') });
  }
  return blocks;
}

function checkActionPin(entry, enforceSha) {
  if (entry.local) return finding('action-pin', 'pass', 'local action path', entry);
  if (!entry.ref) return finding('action-pin', 'fail', 'third-party action is missing an explicit ref', entry);
  if (FULL_SHA.test(entry.ref)) return finding('action-pin', 'pass', 'third-party action is full SHA pinned', entry);
  if (BRANCH_LIKE.test(entry.ref)) return finding('action-pin', 'fail', 'third-party action is pinned to a mutable branch-like ref', entry);
  return finding(
    'action-pin',
    enforceSha ? 'fail' : 'warn',
    'third-party action is tag-pinned, not full-length SHA-pinned',
    entry,
  );
}

function checkInlineExpressions(block) {
  const findings = [];
  const pattern = /\$\{\{\s*([^}]+?)\s*\}\}/g;
  for (const match of block.body.matchAll(pattern)) {
    const expression = match[1].trim();
    if (!UNTRUSTED_INLINE_EXPRESSION.test(expression)) {
      findings.push(finding('workflow-inline-expression', 'warn', 'GitHub expression appears in inline shell; review or move through env', { ...block, expression }));
      continue;
    }
    findings.push(finding('workflow-inline-expression', 'fail', 'untrusted GitHub expression is interpolated directly in inline shell; move through env first', { ...block, expression }));
  }
  return findings;
}

function checkWorkflowPermissions(repoRoot, filePath, text) {
  const file = rel(repoRoot, filePath);
  if (!/^permissions:\s*$/m.test(text)) return [finding('workflow-explicit-permissions', 'fail', 'workflow must declare explicit top-level permissions', { file })];
  if (/permissions:\s*(write-all|read-all)\s*$/m.test(text)) return [finding('workflow-explicit-permissions', 'fail', 'workflow must not use write-all/read-all shorthand permissions', { file })];
  return [finding('workflow-explicit-permissions', 'pass', 'workflow declares explicit top-level permissions', { file })];
}

function checkNpmCiCommands(repoRoot, filePath, text) {
  const file = rel(repoRoot, filePath);
  const findings = [];
  const lines = text.split(/\r?\n/);
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    if (!/\bnpm\s+ci\b/.test(line)) continue;
    const hasLockedSafeInstall =
      /\bnpm\s+ci\b/.test(line)
      && /--ignore-scripts\b/.test(line)
      && /--no-audit\b/.test(line)
      && /--no-fund\b/.test(line)
      && /--omit=optional\b/.test(line);
    findings.push(finding(
      'workflow-npm-ci-locked-tooling',
      hasLockedSafeInstall ? 'pass' : 'fail',
      hasLockedSafeInstall
        ? 'npm ci installs locked dev tooling without lifecycle scripts or unpublished native optionals'
        : 'npm ci must use --ignore-scripts --no-audit --no-fund --omit=optional',
      { file, line: index + 1 },
    ));
  }
  return findings;
}

function publishTokenFree(text) {
  const tokenReference = /\b(?:NPM_TOKEN|NODE_AUTH_TOKEN|NPM_CONFIG_[A-Z0-9_]*TOKEN)\b/i;
  return !tokenReference.test(text);
}

function checkPublishWorkflow(repoRoot) {
  const file = '.github/workflows/publish-npm.yml';
  const text = readTextIfExists(path.join(repoRoot, file));
  if (!text) return [finding('publish-workflow-exists', 'fail', 'publish-npm.yml is missing', { file })];
  return [
    finding('publish-uses-oidc', /id-token:\s*write/.test(text) ? 'pass' : 'fail', 'publish workflow should request id-token: write for npm trusted publishing', { file }),
    finding('publish-no-long-lived-npm-token', publishTokenFree(text) ? 'pass' : 'fail', 'publish workflow should authenticate through npm trusted publishing OIDC without token env fallback', { file }),
    finding('publish-branch-channels-planned', /branches:\s*\n\s*-\s*main\s*\n\s*-\s*master\s*\n\s*-\s*dev/.test(text) && /plan-npm-publish\.mjs --github-output/.test(text) && /needs\.publish-plan\.outputs\.should_publish == 'true'/.test(text) ? 'pass' : 'fail', 'publish workflow should route main/master to stable latest, dev to prerelease dev, and skip already-published versions through the publish plan', { file }),
    finding('publish-protected-environment', /environment:\s*npm-publish/.test(text) ? 'pass' : 'fail', 'publish job should use a protected npm-publish environment', { file }),
    finding('publish-native-contract-enforced', /verify-npm-publish-contract\.mjs --enforce/.test(text) ? 'pass' : 'fail', 'publish must enforce native package contract before npm publish', { file }),
    finding('publish-no-pr-trigger', /^\s*pull_request\s*:/m.test(text) || /^\s*workflow_run\s*:/m.test(text) ? 'fail' : 'pass', 'publish workflow must not be triggerable from pull_request or workflow_run', { file }),
    finding('publish-npm-version-supports-trusted-publishing', /npm@(?:1[1-9]|[2-9][0-9])\./.test(text) ? 'pass' : 'fail', 'publish workflow should pin npm >= 11.x for trusted publishing/OIDC support', { file }),
  ];
}

function checkReleaseAttestation(repoRoot) {
  const file = '.github/workflows/release.yml';
  const text = readTextIfExists(path.join(repoRoot, file));
  if (!text) return [finding('release-workflow-exists', 'fail', 'release.yml is missing', { file })];
  return [
    finding('release-attestations-permission', /attestations:\s*write/.test(text) && /id-token:\s*write/.test(text) ? 'pass' : 'fail', 'release workflow should request id-token: write and attestations: write', { file }),
    finding('release-attestation-step', /uses:\s*actions\/attest@/.test(text) ? 'pass' : 'fail', /uses:\s*actions\/attest@/.test(text) ? 'release workflow generates artifact attestations' : 'release workflow should generate artifact attestations for release assets', { file }),
  ];
}

function run() {
  const args = parseArgs(process.argv.slice(2));
  const files = workflowFiles(args.repoRoot);
  const findings = [];
  findings.push(finding('workflow-files-present', files.length > 0 ? 'pass' : 'fail', `${files.length} workflow files discovered`));

  for (const filePath of files) {
    const text = fs.readFileSync(filePath, 'utf8');
    findings.push(...checkWorkflowPermissions(args.repoRoot, filePath, text));
    findings.push(...checkNpmCiCommands(args.repoRoot, filePath, text));
    for (const entry of extractUsesEntries(args.repoRoot, filePath, text)) findings.push(checkActionPin(entry, args.enforceSha));
    for (const block of extractRunBlocks(args.repoRoot, filePath, text)) findings.push(...checkInlineExpressions(block));
  }
  findings.push(...checkPublishWorkflow(args.repoRoot));
  findings.push(...checkReleaseAttestation(args.repoRoot));

  const failures = findings.filter((item) => item.status === 'fail');
  const warnings = findings.filter((item) => item.status === 'warn');
  const report = {
    status: failures.length > 0 ? 'fail' : warnings.length > 0 ? 'warn' : 'pass',
    checkedAt: new Date().toISOString(),
    repoRoot: '.',
    workflowFiles: files.map((file) => rel(args.repoRoot, file)),
    failures: failures.length,
    warnings: warnings.length,
    findings,
  };

  if (args.json) console.log(JSON.stringify(report, null, 2));
  else {
    console.log(`${report.status}: ${findings.length} workflow policy checks, ${failures.length} failures, ${warnings.length} warnings`);
    for (const item of findings) console.log(`- ${item.status}: ${item.id} — ${item.detail}`);
  }
  process.exitCode = failures.length === 0 ? 0 : 1;
}

try {
  run();
} catch (error) {
  console.error(error?.stack ?? String(error));
  process.exitCode = 1;
}
