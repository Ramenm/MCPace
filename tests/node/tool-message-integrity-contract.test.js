import assert from 'node:assert/strict';
import { test } from 'node:test';
import { collectToolMessageIntegrityAudit } from '../../scripts/tool-message-integrity-audit.mjs';

test('tool message integrity audit passes', () => {
  const report = collectToolMessageIntegrityAudit();
  assert.equal(report.status, 'pass');
  assert.equal(report.summary.failures, 0);
  assert.ok(report.summary.checks >= 16);
});
