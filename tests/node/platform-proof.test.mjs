import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import test from 'node:test';

const repoRoot = path.resolve(import.meta.dirname, '..', '..');

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

function readJson(relativePath) {
  return JSON.parse(read(relativePath));
}

function runPlatformProofJson() {
  const result = spawnSync(process.execPath, ['scripts/platform-proof.mjs', '--json'], {
    cwd: repoRoot,
    encoding: 'utf8',
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);
  return JSON.parse(result.stdout);
}

test('platform proof covers Linux macOS and Windows with native smoke gates', () => {
  const report = runPlatformProofJson();
  assert.equal(report.schema, 'mcpace.platformProof.v1');
  assert.equal(report.overall, 'pass');
  assert.deepEqual(report.platforms.published, ['darwin', 'linux', 'win32']);
  assert.deepEqual(report.platforms.workflow, ['darwin', 'linux', 'win32']);
  assert.ok(report.summary.publishedTargetCount >= 6);
  assert.ok(report.summary.publicCommandCount >= 20);
  assert.ok(report.summary.smokeCommandCount >= 15);

  const smokeCommands = new Set(report.smokeCommands.map((item) => item.command));
  for (const command of [
    'doctor --json',
    'verify readiness --json',
    'server list --json',
    'server capabilities --json',
    'client list --json',
    'hub status --json',
    'lab report --json',
  ]) {
    assert.ok(smokeCommands.has(command), `missing smoke command ${command}`);
  }

  assert.match(report.uiDecision.decision, /Tauri/i);
  assert.match(report.uiDecision.nextTuiGate, /Ratatui/i);
});

test('platform proof workflow is manual and runs Node Rust and binary smoke on all desktop OS families', () => {
  const workflow = read('.github/workflows/platform-proof.yml');
  assert.match(workflow, /workflow_dispatch/);
  assert.match(workflow, /ubuntu-latest/);
  assert.match(workflow, /macos-latest/);
  assert.match(workflow, /windows-latest/);
  assert.match(workflow, /npm run check:platform/);
  assert.match(workflow, /npm run check/);
  assert.match(workflow, /cargo fmt --check/);
  assert.match(workflow, /cargo clippy --all-targets -- -D warnings/);
  assert.match(workflow, /cargo test/);
  assert.match(workflow, /cargo build --release/);
  assert.match(workflow, /npm run platform:binary-smoke/);
});

test('platform proof scripts and reports are part of package checks and release bundle', () => {
  const packageJson = readJson('package.json');
  assert.match(packageJson.scripts.platform, /platform-proof\.mjs --write/);
  assert.match(packageJson.scripts['check:platform'], /platform-proof\.mjs --check/);
  assert.match(packageJson.scripts['platform:binary-smoke'], /platform-binary-smoke\.mjs/);
  assert.match(packageJson.scripts.check, /check:platform/);

  const manifest = readJson('release-manifest.json');
  for (const required of [
    'scripts/platform-proof.mjs',
    'scripts/platform-binary-smoke.mjs',
    'reports/platform-proof.md',
    'reports/platform-proof.json',
  ]) {
    assert.ok(manifest.includePaths.includes(required), `release manifest missing ${required}`);
  }
});
