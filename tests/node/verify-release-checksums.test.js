const test = require('node:test');
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const { repoRoot } = require('./helpers');

async function loadChecksumVerifier() {
  return import(pathToFileURL(path.join(repoRoot, 'scripts', 'verify-release-checksums.mjs')).href);
}

function sha256(value) {
  return crypto.createHash('sha256').update(value).digest('hex');
}

test('release checksum verifier accepts release-upload paths for downloaded npm tarballs', async () => {
  const { verifyReleaseChecksums } = await loadChecksumVerifier();
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-release-checksums-'));
  const tarball = path.join(tmp, 'mcpace-cli-0.3.5.tgz');
  const body = Buffer.from('tarball bytes\n');
  fs.writeFileSync(tarball, body);
  fs.writeFileSync(
    path.join(tmp, 'SHA256SUMS.txt'),
    `${sha256(body)}  dist/release-upload/mcpace-cli-0.3.5.tgz\n${sha256('ignored')}  mcpace-linux-x64-gnu\n`,
    'utf8'
  );

  const report = verifyReleaseChecksums({ artifactDir: tmp });
  assert.equal(report.status, 'pass');
  assert.equal(report.checkedCount, 1);
  assert.equal(report.verified[0].status, 'pass');
});

test('release checksum verifier rejects tampered or uncovered npm tarballs', async () => {
  const { verifyReleaseChecksums } = await loadChecksumVerifier();
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-release-checksums-'));
  fs.writeFileSync(path.join(tmp, 'mcpace-cli-0.3.5.tgz'), 'tampered\n', 'utf8');
  fs.writeFileSync(path.join(tmp, 'extra-package-0.3.5.tgz'), 'extra\n', 'utf8');
  fs.writeFileSync(
    path.join(tmp, 'SHA256SUMS.txt'),
    `${sha256('original\n')}  dist/release-upload/mcpace-cli-0.3.5.tgz\n`,
    'utf8'
  );

  const report = verifyReleaseChecksums({ artifactDir: tmp });
  assert.equal(report.status, 'fail');
  assert.match(report.issues.join('\n'), /checksum mismatch/);
  assert.match(report.issues.join('\n'), /extra-package-0\.3\.5\.tgz/);
});
