const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { cleanChildEnv, packageVersion, repoRoot } = require('./helpers');

function runArchiveBuilder(outputDir, stamp) {
  return spawnSync(
    process.execPath,
    [path.join('scripts', 'archive-release.mjs'), '--json', '--output-dir', outputDir, '--stamp', stamp],
    {
      cwd: repoRoot,
      encoding: 'utf8',
      env: cleanChildEnv()
    }
  );
}

function listArchiveEntries(archivePath) {
  if (process.platform === 'win32') {
    const escapedArchivePath = archivePath.replace(/'/g, "''");
    const listing = spawnSync(
      'powershell.exe',
      [
        '-NoProfile',
        '-Command',
        [
          'Add-Type -AssemblyName System.IO.Compression.FileSystem',
          `$archive = [System.IO.Compression.ZipFile]::OpenRead('${escapedArchivePath}')`,
          'try { $archive.Entries | ForEach-Object { $_.FullName } } finally { $archive.Dispose() }'
        ].join('; ')
      ],
      {
        encoding: 'utf8',
        env: cleanChildEnv()
      }
    );
    assert.equal(
      listing.status,
      0,
      listing.stderr || listing.error?.message || listing.stdout
    );
    return listing.stdout.trim().split(/\r?\n/).filter(Boolean);
  }

  const listing = spawnSync('unzip', ['-Z1', archivePath], {
    encoding: 'utf8',
    env: cleanChildEnv()
  });
  assert.equal(listing.status, 0, listing.stderr);
  return listing.stdout.trim().split(/\r?\n/).filter(Boolean);
}

test('archive builder creates a clean zip with the required root naming contract', () => {
  const outputDir = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-archive-contract-'));
  const stamp = '190426-235959';
  const version = packageVersion();
  const escapedVersion = version.replace(/\./g, '\\.');
  const result = runArchiveBuilder(outputDir, stamp);
  assert.equal(result.status, 0, result.stderr);

  const report = JSON.parse(result.stdout);
  assert.equal(report.projectName, 'mcpace');
  assert.equal(report.version, version);
  assert.equal(report.stamp, stamp);
  assert.deepEqual(report.includedOptionalPaths, []);
  assert.match(report.rootName, new RegExp(`^mcpace-v${escapedVersion}-${stamp}$`));
  assert.match(report.archiveName, new RegExp(`^mcpace-v${escapedVersion}-${stamp}\\.zip$`));
  assert.equal(fs.existsSync(report.archivePath), true, report.archivePath);

  const files = listArchiveEntries(report.archivePath);

  assert.ok(files.includes(`${report.rootName}/docs/README.md`));
  assert.ok(files.includes(`${report.rootName}/reports/summary.md`));
  assert.ok(files.includes(`${report.rootName}/TODO.md`));
  assert.ok(files.includes(`${report.rootName}/STATE.md`));
  assert.ok(files.includes(`${report.rootName}/DECISIONS.md`));
  assert.ok(files.includes(`${report.rootName}/scripts/archive-release.mjs`));
  assert.ok(files.includes(`${report.rootName}/scripts/verify-vendored-binary.mjs`));
  assert.ok(files.includes(`${report.rootName}/packages/npm/cli/bin/mcpace.js`));
  assert.ok(files.every((entry) => !entry.includes('/node_modules/')));
  assert.ok(files.every((entry) => !entry.includes('/.git/')));
  assert.ok(files.every((entry) => !entry.includes('/target/')));
  assert.ok(files.every((entry) => !entry.endsWith('.DS_Store')));
  assert.ok(files.every((entry) => !entry.startsWith('__MACOSX/')));
});
