const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const repoRoot = path.resolve(__dirname, '..', '..');

function runNode(args) {
  const result = spawnSync('node', args, { cwd: repoRoot, encoding: 'utf8' });
  assert.equal(result.status, 0, `${args.join(' ')} failed\nSTDOUT:\n${result.stdout}\nSTDERR:\n${result.stderr}`);
  return result;
}

test('project inventory maps source inventory to the legacy code inventory shape', () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-code-inventory-'));
  const jsonPath = path.join(tmpDir, 'code-inventory.json');
  const mdPath = path.join(tmpDir, 'code-inventory.md');
  const result = runNode(['scripts/inventory-project.mjs', '--json', '--write-json', jsonPath, '--write-md', mdPath]);
  const report = JSON.parse(result.stdout);
  const rootPkg = JSON.parse(fs.readFileSync(path.join(repoRoot, 'package.json'), 'utf8'));

  assert.equal(report.schema, 'mcpace.codeInventory.v2');
  assert.equal(report.version, rootPkg.version);
  assert.ok(report.counts.totalFiles > 300);
  assert.ok(report.counts.rustFiles > 80);
  assert.ok(report.counts.nodeFiles > 50);
  assert.ok(report.largestRustFiles.some((entry) => entry.path === 'src/serve.rs' || entry.path.startsWith('src/')));
  assert.ok(report.topDirectories.some((entry) => entry.directory === 'src'));
  assert.equal(JSON.parse(fs.readFileSync(jsonPath, 'utf8')).schema, 'mcpace.codeInventory.v2');
  assert.match(fs.readFileSync(mdPath, 'utf8'), /Largest Rust files/);
});
