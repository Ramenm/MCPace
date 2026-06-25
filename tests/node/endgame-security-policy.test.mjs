import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import test from 'node:test';
import { readRootPackageJson, repoRoot } from '../../scripts/lib/project-metadata.mjs';

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

test('security policy check is wired into CI and remains hard-green', () => {
  const packageJson = readRootPackageJson();
  assert.match(packageJson.scripts['check:security-policy'], /security-policy-check\.mjs/);
  assert.match(packageJson.scripts['check:ci'], /check:security-policy/);

  const output = execFileSync(process.execPath, ['scripts/security-policy-check.mjs'], {
    cwd: repoRoot,
    encoding: 'utf8',
    maxBuffer: 4 * 1024 * 1024,
  });
  const report = JSON.parse(output);
  assert.equal(report.schema, 'mcpace.securityPolicyCheck.v1');
  assert.equal(report.hardFailures, 0);
  assert.ok(['pass', 'warn'].includes(report.status));
  for (const id of [
    'github-actions-no-direct-expressions-in-run',
    'npm-package-has-no-install-lifecycle-scripts',
    'workflows-use-ignore-scripts-for-npm-install',
    'redos-policy-no-dynamic-or-nested-regex-in-production',
    'local-http-ssrf-and-csrf-boundaries-present',
    'native-resolver-spoofing-and-toctou-guards-present',
    'npm-publish-workflow-trusted-publishing-shape',
  ]) {
    const check = report.checks.find((entry) => entry.id === id);
    assert.equal(check?.status, 'pass', `${id} should pass`);
  }
});

test('native resolver explicitly rejects relative binary override and symlinked metadata classes', () => {
  const resolver = read('packages/npm/cli/lib/resolve-binary.js');
  assert.match(resolver, /must be an absolute path, not a cwd-relative binary override/);
  assert.match(resolver, /readRegularTextFileStable/);
  assert.match(resolver, /O_NOFOLLOW/);
  assert.match(resolver, /changed while being read/);
  assert.match(resolver, /must not be a symbolic link/);
});

test('local HTTP and upstream guards cover SSRF/CSRF-adjacent local-boundary classes', () => {
  const boundary = read('src/dashboard/http_boundary.rs');
  const dashboard = read('src/dashboard.rs');
  const upstream = read('src/upstream/http_runtime.rs');
  assert.match(boundary, /missing required Host header/);
  assert.match(boundary, /multiple Host headers are not allowed/);
  assert.match(boundary, /is_allowed_local_origin/);
  assert.match(boundary, /is_loopback_host/);
  assert.match(dashboard, /refusing to bind non-loopback host/);
  assert.match(dashboard, /must be a local file path, not a remote URL/);
  assert.match(upstream, /HTTP upstream URL cannot be empty or contain whitespace\/control characters/);
  assert.doesNotMatch(upstream, /let\s+body_bytes\s*=\s*if\s+parsed\s*\n\s*let\s+body_bytes\s*=\s*if\s+parsed/);
});
