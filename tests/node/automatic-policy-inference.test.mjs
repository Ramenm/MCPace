import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';

test('declared servers without policy still inherit generic source policy inference', () => {
  const loader = fs.readFileSync(path.join(repoRoot, 'src', 'server', 'loader.rs'), 'utf8');
  const normalizeStart = loader.indexOf('fn normalize_server_record(');
  const normalizeEnd = loader.indexOf('\nfn policy_string(', normalizeStart);
  assert.notEqual(normalizeStart, -1);
  assert.notEqual(normalizeEnd, -1);
  const normalizeBody = loader.slice(normalizeStart, normalizeEnd);

  assert.match(normalizeBody, /let\s+inferred_policy\s*=\s*infer_generic_source_policy\(/);
  assert.match(normalizeBody, /policy_string\(policy,\s*"scopeClass",\s*inferred_policy\.scope_class\)/);
  assert.match(normalizeBody, /policy_string\(\s*policy,\s*"concurrencyPolicy",\s*inferred_policy\.concurrency_policy/s);
  assert.match(normalizeBody, /policy_usize\(\s*policy,\s*"parallelismLimit",\s*inferred_policy\.parallelism_limit/s);
  assert.doesNotMatch(normalizeBody, /policy_string\(policy,\s*"scopeClass",\s*""\)/);
  assert.doesNotMatch(normalizeBody, /policy_string\(policy,\s*"concurrencyPolicy",\s*""\)/);
});
