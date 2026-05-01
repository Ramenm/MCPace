const test = require('node:test');
const assert = require('node:assert/strict');
const { read } = require('./helpers.js');

test('Node coverage lane is available without adding a third-party test dependency', () => {
  const packageJson = JSON.parse(read('package.json'));
  const testStrategy = read('docs/test-strategy.md');
  const verification = read('docs/verification-matrix.md');

  assert.equal(
    packageJson.scripts['test:node:coverage'],
    'node --test --experimental-test-coverage tests/node/*.test.js packages/npm/cli/test/*.test.mjs',
  );
  assert.match(packageJson.scripts['lint:npm'], /coverage-contract\.test\.js/);
  assert.match(testStrategy, /test:node:coverage/);
  assert.match(verification, /Node coverage/);

  // The command should stay repo-local and should not depend on nyc/c8/jest.
  assert.equal(packageJson.devDependencies, undefined);
});
