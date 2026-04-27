const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { cleanChildEnv, repoRoot } = require('./helpers');

const checksumScript = path.join(repoRoot, 'scripts', 'generate-checksums.mjs');

test('generate-checksums hashes nested files when recursive mode is enabled', () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-checksums-'));
  const nestedDir = path.join(tmp, 'nested');
  fs.mkdirSync(nestedDir, { recursive: true });
  fs.writeFileSync(path.join(tmp, 'alpha.txt'), 'alpha\n', 'utf8');
  fs.writeFileSync(path.join(nestedDir, 'beta.txt'), 'beta\n', 'utf8');

  const result = spawnSync(
    process.execPath,
    [checksumScript, '--json', '--output-dir', tmp, '--output-path', path.join(tmp, 'SHA256SUMS.txt'), '--recursive'],
    {
      cwd: repoRoot,
      encoding: 'utf8',
      env: cleanChildEnv()
    }
  );

  assert.equal(result.status, 0, result.stderr);
  const report = JSON.parse(result.stdout);
  assert.equal(report.fileCount, 2);
  assert.equal(report.recursive, true);
  assert.deepEqual(
    report.entries.map((entry) => entry.name),
    ['alpha.txt', 'nested/beta.txt']
  );
  const body = fs.readFileSync(path.join(tmp, 'SHA256SUMS.txt'), 'utf8');
  assert.match(body, /alpha\.txt/);
  assert.match(body, /nested\/beta\.txt/);
});
