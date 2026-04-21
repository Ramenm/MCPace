import test from 'node:test';
import assert from 'node:assert/strict';
import { SUPPORTED_TARGETS, currentTargetKey, detectTarget, packageNamesForTarget, describeSupportedTargets } from '../lib/platform.js';

test('supported targets include linux and windows lanes', () => {
  assert.ok(SUPPORTED_TARGETS.some((entry) => entry.key === 'linux-x64-gnu'));
  assert.ok(SUPPORTED_TARGETS.some((entry) => entry.key === 'win32-x64-msvc'));
});

test('packageNamesForTarget maps to platform package names', () => {
  assert.deepEqual(packageNamesForTarget({ key: 'linux-x64-gnu' }), ['@mcpace/cli-linux-x64-gnu']);
});

test('currentTargetKey preserves libc information even when the target is unsupported', () => {
  assert.equal(currentTargetKey('linux', 'x64', 'musl'), 'linux-x64-musl');
  assert.equal(detectTarget('linux', 'x64', 'musl'), null);
});

test('describeSupportedTargets is non-empty and current target detection is safe', () => {
  assert.match(describeSupportedTargets(), /linux|darwin|win32/);
  const target = detectTarget();
  if (process.platform === 'linux' && process.arch === 'x64') {
    assert.ok(target === null || target.key === 'linux-x64-gnu');
  }
});
