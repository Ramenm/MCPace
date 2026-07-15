import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

test('server install command shell-composition guard is shared by dashboard and CLI planner', () => {
  const textUtils = read('src/text_utils.rs');
  const dashboard = read('src/dashboard.rs');
  const autoinstall = read('src/mcp_autoinstall.rs');
  const autoinstallTests = read('src/mcp_autoinstall/tests.rs');

  assert.match(textUtils, /fn uses_shell_composition\(value: &str\) -> bool/);
  assert.match(textUtils, /matches!\(ch, '`' \| ';' \| '\|' \| '<' \| '>' \| '&'\)/);
  assert.match(textUtils, /ch == '\$' && chars\.get\(index \+ 1\) == Some\(&'\('\)/);
  assert.match(dashboard, /fn command_line_uses_shell_composition\(value: &str\) -> bool \{\n\s+text_utils::uses_shell_composition\(value\)/);
  assert.match(autoinstall, /text_utils::uses_shell_composition\(value\)/);
  assert.match(autoinstall, /background operators/);
  assert.match(autoinstallTests, /command_like_install_rejects_shell_composition/);
});
