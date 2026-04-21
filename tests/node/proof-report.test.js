const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const repoRoot = path.resolve(__dirname, '..', '..');
const proofScript = path.join(repoRoot, 'scripts', 'proof-report.mjs');

function readJson(relativePath) {
  return JSON.parse(fs.readFileSync(path.join(repoRoot, relativePath), 'utf8'));
}

test('proof report cli emits a deterministic no-run report', () => {
  const result = spawnSync(
    process.execPath,
    [proofScript, '--json', '--no-run', '--checked-at', '2026-04-19T21:00:00Z'],
    {
      cwd: repoRoot,
      encoding: 'utf8'
    }
  );

  assert.equal(result.status, 0, result.stderr);
  const report = JSON.parse(result.stdout);
  assert.equal(report.version, readJson('package.json').version);
  assert.equal(report.checkedAt, '2026-04-19T21:00:00Z');
  assert.equal(report.environment.node, process.version);
  assert.equal(report.sourceProof.status, 'not-run');
  assert.equal(report.releaseProof.status, 'not-run');
  assert.ok(['blocked', 'not-run'].includes(report.buildProof.status));
  assert.ok(['blocked', 'not-run'].includes(report.runtimeProof.status));
});

test('proof report module can write a report file', async () => {
  const { collectReport, writeReport } = await import(pathToFileUrl(proofScript));
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-proof-report-'));
  const outputPath = path.join(tmp, 'verification-latest.json');

  try {
    const report = collectReport({ noRun: true, checkedAt: '2026-04-19T21:05:00Z' });
    const writtenPath = writeReport(report, outputPath);
    assert.equal(writtenPath, outputPath);
    const written = JSON.parse(fs.readFileSync(outputPath, 'utf8'));
    assert.equal(written.checkedAt, '2026-04-19T21:05:00Z');
    assert.equal(written.sourceProof.reason, 'proof commands were skipped via --no-run');
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

function pathToFileUrl(filePath) {
  const resolved = path.resolve(filePath);
  const prefix = process.platform === 'win32' ? 'file:///' : 'file://';
  return `${prefix}${resolved.split(path.sep).join('/')}`;
}
