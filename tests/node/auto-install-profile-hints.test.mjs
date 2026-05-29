import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';

const read = (...parts) => fs.readFileSync(path.join(repoRoot, ...parts), 'utf8');

test('direct filesystem installs persist restrictive profile hints for automatic classification', () => {
  const autoinstall = read('src', 'mcp_autoinstall.rs');
  assert.match(autoinstall, /fn profile_hints_for_plan\(/);
  assert.match(autoinstall, /fn plan_is_explicit_filesystem\(/);
  assert.match(autoinstall, /plan\.method == "filesystem-path"/);
  assert.match(autoinstall, /has_filesystem_path_argument\(plan\)/);
  assert.match(autoinstall, /"filesystem"\.to_string\(\)/);
  assert.match(autoinstall, /"project-root"\.to_string\(\)/);
  assert.match(autoinstall, /"isolated-per-project"\.to_string\(\)/);
  assert.match(autoinstall, /profile_hints\.extend\(profile_hints_for_plan\(&plan\)\)/);

  const writeHelpers = read('src', 'mcp_sources', 'write_helpers.rs');
  assert.match(writeHelpers, /"mcpaceProfileHints"/);

  const loader = read('src', 'server', 'loader.rs');
  assert.match(loader, /mcpaceProfileHints/);
  assert.match(loader, /source_signal_args/);
});

test('direct install hints are restrictive intent, not a name-only relaxation path', () => {
  const autoinstall = read('src', 'mcp_autoinstall.rs');
  const helperStart = autoinstall.indexOf('fn profile_hints_for_plan(');
  const helperEnd = autoinstall.indexOf('\nfn filesystem_path_install_plan', helperStart);
  assert.notEqual(helperStart, -1);
  assert.notEqual(helperEnd, -1);
  const helper = autoinstall.slice(helperStart, helperEnd);

  assert.match(helper, /tighten/);
  assert.match(helper, /requires initialize\/tools-list evidence before widening concurrency/);
  assert.match(helper, /plan_is_explicit_filesystem\(plan\)/);
  assert.doesNotMatch(helper, /plan\.name/);
  assert.doesNotMatch(helper, /plan\.original_spec/);
});
