import test from 'node:test';
import assert from 'node:assert/strict';
import {
  MINIMUM_NODE_MAJOR,
  SUPPORTED_NODE_LTS_MAJORS,
  assertSupportedNodeVersion,
  formatUnsupportedNodeMessage,
  isSupportedNodeMajor,
  parseNodeMajor
} from '../lib/runtime.js';

test('runtime launcher policy keeps Node 22+ as the supported floor', () => {
  assert.equal(MINIMUM_NODE_MAJOR, 22);
  assert.deepEqual(SUPPORTED_NODE_LTS_MAJORS, [22, 24]);
  assert.equal(parseNodeMajor('24.15.0'), 24);
  assert.equal(parseNodeMajor('22.12.1'), 22);
  assert.equal(Number.isNaN(parseNodeMajor('not-a-version')), true);
  assert.equal(isSupportedNodeMajor(24), true);
  assert.equal(isSupportedNodeMajor(22), true);
  assert.equal(isSupportedNodeMajor(21), false);
});

test('runtime launcher rejects unsupported Node majors with a clear message', () => {
  assert.throws(
    () => assertSupportedNodeVersion('18.19.0'),
    (error) => {
      assert.match(error.message, /requires Node\.js 22 or newer/i);
      assert.equal(error.code, 'MCPACE_UNSUPPORTED_NODE');
      return true;
    }
  );
  assert.match(formatUnsupportedNodeMessage('18.19.0'), /Node 22 or Node 24 LTS/);
});

test('runtime launcher accepts maintained lanes', () => {
  assert.equal(assertSupportedNodeVersion('22.9.0'), 22);
  assert.equal(assertSupportedNodeVersion('24.15.0'), 24);
});
