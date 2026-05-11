const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const repoRoot = path.resolve(__dirname, '..', '..');

function runNode(args, options = {}) {
  const result = spawnSync('node', args, {
    cwd: repoRoot,
    encoding: 'utf8',
    timeout: 60_000,
    maxBuffer: 4 * 1024 * 1024,
    ...options,
  });
  assert.equal(result.status, 0, `${args.join(' ')} failed\nSTDOUT:\n${result.stdout}\nSTDERR:\n${result.stderr}`);
  return result;
}

function readJson(relativePath) {
  return JSON.parse(fs.readFileSync(path.join(repoRoot, relativePath), 'utf8'));
}

test('source inventory produces a deterministic first-use and release manifest report', () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-inventory-'));
  const jsonPath = path.join(tmpDir, 'inventory.json');
  const mdPath = path.join(tmpDir, 'inventory.md');
  const result = runNode(['scripts/inventory-source.mjs', '--json', '--write', jsonPath, '--markdown', mdPath]);
  const report = JSON.parse(result.stdout);

  assert.equal(report.schema, 'mcpace.sourceInventory.v1');
  assert.equal(report.ok, true, JSON.stringify(report.warnings));
  assert.ok(report.summary.totalFiles > 300);
  assert.ok(report.summary.rustFiles > 50);
  assert.ok(report.summary.nodeFiles > 20);
  assert.ok(report.largestRustModules.some((entry) => entry.path.startsWith('src/')));
  assert.equal(report.versions.drift.length, 0);
  assert.equal(report.releaseManifest.missing.length, 0);
  assert.equal(report.presets.status, 'ok');
  assert.ok(report.presets.ids.includes('filesystem'));

  assert.equal(JSON.parse(fs.readFileSync(jsonPath, 'utf8')).schema, 'mcpace.sourceInventory.v1');
  assert.match(fs.readFileSync(mdPath, 'utf8'), /MCPace source inventory/);
});

test('boot harness summarizes install readiness without mutating the tree', () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-boot-'));
  const jsonPath = path.join(tmpDir, 'boot.json');
  const result = runNode(['scripts/boot-harness.mjs', '--json', '--skip-npm-pack', '--write', jsonPath]);
  const report = JSON.parse(result.stdout);
  const rootPkg = readJson('package.json');

  assert.equal(report.schema, 'mcpace.bootHarness.v1');
  assert.equal(report.project.version, rootPkg.version);
  assert.equal(report.inventory.schema, 'mcpace.sourceInventory.v1');
  assert.equal(report.inventory.ok, true);
  assert.equal(report.sourceAudit.status, 'pass');
  assert.equal(report.nodeSyntax.status, 'pass');
  assert.ok(report.nodeSyntax.fileCount > 20);
  assert.equal(report.npmPack.status, 'skipped');
  const npmProbe = process.platform === 'win32'
    ? spawnSync('cmd.exe', ['/d', '/s', '/c', 'npm', '--version'], { cwd: repoRoot, encoding: 'utf8' })
    : spawnSync('npm', ['--version'], { cwd: repoRoot, encoding: 'utf8' });
  if (npmProbe.status === 0) {
    assert.equal(report.toolchain.npm.available, true, report.toolchain.npm.error || 'npm should be detected when available to the invoking shell');
    assert.equal(report.toolchain.npm.supported, true, `npm ${report.toolchain.npm.versionText} should satisfy ${rootPkg.engines.npm}`);
  }
  assert.match(report.binaryDistribution.mode, /thin-launcher|vendored-binary-bundle|platform-binary-packages/);
  assert.ok(['pass', 'partial', 'blocked'].includes(report.installReadiness.status));
  assert.ok(report.nextActions.some((entry) => entry.includes('cargo check --all-targets --locked')));
  assert.equal(JSON.parse(fs.readFileSync(jsonPath, 'utf8')).schema, 'mcpace.bootHarness.v1');
});

test('package scripts expose inventory and boot harness commands', () => {
  const rootPkg = readJson('package.json');
  assert.equal(rootPkg.scripts['inventory:source'], 'node scripts/inventory-source.mjs --json --write reports/code-inventory-latest.json --markdown reports/code-inventory-latest.md');
  assert.equal(rootPkg.scripts['verify:boot'], 'node scripts/boot-harness.mjs --json --write reports/boot-harness-latest.json --markdown reports/boot-harness-latest.md');
  assert.equal(rootPkg.scripts['lint:npm'], 'node scripts/check-node-syntax.mjs --json');
  assert.equal(rootPkg.scripts['lint:node'], 'node scripts/check-node-syntax.mjs --json');
});
