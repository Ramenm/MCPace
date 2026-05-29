import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';

function walkRustFiles(dir) {
  const files = [];
  const stack = [dir];
  while (stack.length > 0) {
    const current = stack.pop();
    for (const entry of fs.readdirSync(current, { withFileTypes: true })) {
      const full = path.join(current, entry.name);
      if (entry.isDirectory()) {
        stack.push(full);
      } else if (entry.isFile() && entry.name.endsWith('.rs')) {
        files.push(full);
      }
    }
  }
  return files.sort();
}

test('Rust source has no adjacent duplicate match arms', () => {
  const offenders = [];
  for (const file of walkRustFiles(path.join(repoRoot, 'src'))) {
    const lines = fs.readFileSync(file, 'utf8').split(/\r?\n/);
    for (let index = 0; index < lines.length - 1; index += 1) {
      const current = lines[index].trim();
      const next = lines[index + 1].trim();
      if (current === next && current.includes('=>')) {
        offenders.push(`${path.relative(repoRoot, file)}:${index + 1}:${current}`);
      }
    }
  }
  assert.deepEqual(offenders, []);
});
