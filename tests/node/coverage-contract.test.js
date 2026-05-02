const test = require('node:test');
const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const { read, repoRoot } = require('./helpers.js');

test('Node coverage lane is available without adding a third-party test dependency', () => {
  const packageJson = JSON.parse(read('package.json'));
  const testStrategy = read('docs/test-strategy.md');
  const verification = read('docs/verification-matrix.md');

  assert.equal(
    packageJson.scripts['test:node:coverage'],
    'node --test --test-force-exit --experimental-test-coverage tests/node/*.test.js packages/npm/cli/test/*.test.mjs',
  );
  assert.equal(packageJson.scripts['lint:npm'], 'node scripts/check-node-syntax.mjs --json');
  const syntax = spawnSync(process.execPath, ['scripts/check-node-syntax.mjs', '--json', '--list'], { cwd: repoRoot, encoding: 'utf8' });
  assert.equal(syntax.status, 0, syntax.stderr || syntax.stdout);
  assert.ok(JSON.parse(syntax.stdout).files.includes('tests/node/coverage-contract.test.js'));
  assert.match(testStrategy, /test:node:coverage/);
  assert.match(verification, /Node coverage/);

  // The command should stay repo-local and should not depend on nyc/c8/jest.
  assert.equal(packageJson.devDependencies, undefined);
});
