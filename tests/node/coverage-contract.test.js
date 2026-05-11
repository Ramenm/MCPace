const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
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
  assert.equal(fs.existsSync(path.join(repoRoot, 'tests/node/coverage-contract.test.js')), true);
  assert.match(testStrategy, /test:node:coverage/);
  assert.match(verification, /Node coverage/);

  // The command should stay repo-local and should not depend on nyc/c8/jest.
  assert.equal(packageJson.devDependencies, undefined);
});
