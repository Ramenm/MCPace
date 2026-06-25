import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { readRootPackageJson, repoRoot } from '../../scripts/lib/project-metadata.mjs';

test('Cargo keeps a compact release profile and a separate speed-oriented perf profile', () => {
  const cargoToml = fs.readFileSync(path.join(repoRoot, 'Cargo.toml'), 'utf8');
  assert.match(cargoToml, /\[profile\.release\][\s\S]*opt-level\s*=\s*"z"/);
  assert.match(cargoToml, /\[profile\.perf\]/);
  assert.match(cargoToml, /\[profile\.perf\][\s\S]*inherits\s*=\s*"release"/);
  assert.match(cargoToml, /\[profile\.perf\][\s\S]*opt-level\s*=\s*3/);
  assert.equal(readRootPackageJson().scripts['build:perf'], 'node scripts/cargo-task.mjs build --profile perf');
});
