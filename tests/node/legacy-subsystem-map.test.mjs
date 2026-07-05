import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';

function walkFiles(root, predicate = () => true) {
  const files = [];
  const stack = [root];
  while (stack.length > 0) {
    const current = stack.pop();
    if (!fs.existsSync(current)) continue;
    for (const entry of fs.readdirSync(current, { withFileTypes: true }).sort((left, right) => left.name.localeCompare(right.name))) {
      const full = path.join(current, entry.name);
      if (entry.isDirectory()) stack.push(full);
      else if (entry.isFile() && predicate(full)) files.push(full);
    }
  }
  return files.sort();
}

function normalize(relativePath) {
  return relativePath.split(path.sep).join('/');
}

function gitSourceFiles(pattern) {
  const result = spawnSync('git', ['-C', repoRoot, 'ls-files', '-co', '--exclude-standard', '-z', '--', pattern], {
    encoding: 'buffer',
    windowsHide: true,
  });
  if (result.status !== 0) return null;
  return result.stdout
    .toString('utf8')
    .split('\0')
    .filter(Boolean)
    .filter((relative) => {
      try {
        return fs.statSync(path.join(repoRoot, relative)).isFile();
      } catch {
        return false;
      }
    })
    .map(normalize)
    .sort();
}

test('legacy subsystem map reports modernization seams by subsystem', () => {
  const result = spawnSync(process.execPath, ['scripts/legacy-subsystem-map.mjs', '--json'], {
    cwd: repoRoot,
    encoding: 'utf8',
    windowsHide: true,
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);
  const report = JSON.parse(result.stdout);
  assert.equal(report.schema, 'mcpace.legacySubsystemMap.v1');
  assert.ok(report.rustFiles > 100);
  const ids = new Map(report.findings.map((item) => [item.id, item]));
  assert.equal(ids.get('dependencies.compat-crates')?.status, 'done');
  assert.equal(ids.get('source.generated-partials')?.status, 'done');
  assert.ok(['blocked', 'done'].includes(ids.get('dependencies.cargo-lock-refresh')?.status));
  assert.equal(ids.get('cli.manual-argv')?.replacement.includes('clap'), true);
  assert.equal(ids.get('config.lossless-editing')?.replacement.includes('toml_edit'), true);
  assert.equal(ids.get('mcp.stdio-preview')?.replacement.includes('mcpace stdio'), true);
});

test('checked-in eval sweep partial streams are removed from the source tree', () => {
  const offenders = gitSourceFiles('*.partial.jsonl') ?? walkFiles(repoRoot, (file) => file.endsWith('.partial.jsonl'))
    .map((file) => normalize(path.relative(repoRoot, file)))
    .filter((relative) => !relative.startsWith('node_modules/') && !relative.startsWith('.git/') && !relative.startsWith('.omx/'));
  assert.deepEqual(offenders, []);
});
