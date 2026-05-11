const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const repoRoot = path.resolve(__dirname, '..', '..');

function runNode(args) {
  const result = spawnSync('node', args, {
    cwd: repoRoot,
    encoding: 'utf8',
    timeout: 60_000,
    maxBuffer: 4 * 1024 * 1024,
  });
  assert.equal(result.status, 0, `${args.join(' ')} failed\nSTDOUT:\n${result.stdout}\nSTDERR:\n${result.stderr}`);
  return result;
}

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

function readJson(relativePath) {
  return JSON.parse(read(relativePath));
}

test('GitHub readiness harness checks public repository health files and workflows', () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-github-readiness-'));
  const jsonPath = path.join(tmpDir, 'github-readiness.json');
  const mdPath = path.join(tmpDir, 'github-readiness.md');
  const result = runNode([
    'scripts/verify-github-readiness.mjs',
    '--json',
    '--write',
    jsonPath,
    '--markdown',
    mdPath,
  ]);
  const report = JSON.parse(result.stdout);

  assert.equal(report.schema, 'mcpace.githubReadiness.v1');
  assert.notEqual(report.status, 'blocked');
  assert.ok(report.summary.requiredPassed >= 20);
  assert.ok(report.checks.some((entry) => entry.id === 'file:CODE_OF_CONDUCT.md' && entry.status === 'pass'));
  assert.ok(report.checks.some((entry) => entry.id === 'workflow:.github/workflows/security.yml' && entry.status === 'pass'));
  assert.ok(report.checks.some((entry) => entry.id === 'truthful-readme-claims' && entry.status === 'pass'));
  assert.equal(JSON.parse(fs.readFileSync(jsonPath, 'utf8')).schema, 'mcpace.githubReadiness.v1');
  assert.match(fs.readFileSync(mdPath, 'utf8'), /GitHub readiness/i);
});

test('package scripts expose GitHub readiness verification', () => {
  const pkg = readJson('package.json');
  assert.equal(
    pkg.scripts['verify:github-readiness'],
    'node scripts/verify-github-readiness.mjs --json --write reports/github-readiness-latest.json --markdown reports/github-readiness-latest.md'
  );
  assert.match(pkg.scripts['prove:source'], /verify:github-readiness/);

  assert.ok(fs.existsSync(path.join(repoRoot, 'scripts/verify-github-readiness.mjs')));
  assert.ok(fs.existsSync(path.join(repoRoot, 'tests/node/github-readiness-contract.test.js')));
});
