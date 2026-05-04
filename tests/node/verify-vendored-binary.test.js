const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { cleanChildEnv, packageVersion, repoRoot } = require('./helpers');

const verifyScript = path.join(repoRoot, 'scripts', 'verify-vendored-binary.mjs');

function createVendoredBinaryFixture(filePath, version = packageVersion(), options = {}) {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  const rustSourceReady = options.rustSourceReady ?? true;
  const doctorJson = JSON.stringify({
    project: {
      configFound: true,
      cargoManifestFound: options.cargoManifestFound ?? true,
      rustSourceReady,
      npmSurfaceReady: true,
    },
  });

  if (process.platform === 'win32') {
    fs.writeFileSync(
      filePath,
      [
        '@echo off',
        'if "%1"=="version" (',
        `  echo ${version}`,
        '  exit /b 0',
        ')',
        'if "%1"=="help" (',
        '  echo MCPace fixture help',
        '  exit /b 0',
        ')',
        'if "%1"=="verify" if "%2"=="doctor" if "%3"=="--json" (',
        `  echo ${doctorJson.replace(/"/g, '\\"')}`,
        '  exit /b 0',
        ')',
        'if "%1"=="verify" if "%2"=="readiness" if "%3"=="--json" (',
        '  echo {"readyForReadOnlyOps":true,"readyForRuntimeOps":false}',
        '  exit /b 0',
        ')',
        'echo unsupported args',
        'exit /b 1'
      ].join('\r\n'),
      'utf8'
    );
    return filePath;
  }

  fs.writeFileSync(
    filePath,
    [
      '#!/usr/bin/env sh',
      'if [ "$1" = "version" ]; then',
      `  echo ${version}`,
      '  exit 0',
      'fi',
      'if [ "$1" = "help" ]; then',
      '  echo "MCPace fixture help"',
      '  exit 0',
      'fi',
      'if [ "$1" = "verify" ] && [ "$2" = "doctor" ] && [ "$3" = "--json" ]; then',
      `  printf '%s\\n' '${doctorJson}'`,
      '  exit 0',
      'fi',
      'if [ "$1" = "verify" ] && [ "$2" = "readiness" ] && [ "$3" = "--json" ]; then',
      "  printf '%s\\n' '{\"readyForReadOnlyOps\":true,\"readyForRuntimeOps\":false}'",
      '  exit 0',
      'fi',
      'echo "unsupported args" >&2',
      'exit 1'
    ].join('\n'),
    'utf8'
  );
  fs.chmodSync(filePath, 0o755);
  return filePath;
}

test('verify-vendored-binary smoke-checks an explicit binary path', () => {
  if (process.platform === 'win32') {
    return;
  }

  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-verify-vendor-'));
  const binaryPath = createVendoredBinaryFixture(path.join(tmp, 'mcpace-fixture'));
  const version = packageVersion();

  try {
    const result = spawnSync(
      process.execPath,
      [verifyScript, '--json', '--binary-path', binaryPath, '--target-key', 'linux-x64-gnu'],
      {
        cwd: repoRoot,
        encoding: 'utf8',
        env: cleanChildEnv()
      }
    );

    assert.equal(result.status, 0, result.stderr);
    const report = JSON.parse(result.stdout);
    assert.equal(report.schema, 'mcpace.vendoredBinary.v1');
    assert.match(report.generatedAt, /^\d{4}-\d{2}-\d{2}T/);
    assert.equal(report.project.name, 'mcpace');
    assert.equal(report.status, 'pass');
    assert.equal(report.targetKey, 'linux-x64-gnu');
    assert.equal(path.resolve(report.binaryPath), path.resolve(binaryPath));
    assert.equal(report.expectedVersion, version);
    assert.equal(report.binaryVersion, version);
    assert.equal(report.versionOutput, version);
    assert.equal(report.helpMentionsMcpace, true);
    assert.deepEqual(report.doctorSummary, {
      configFound: true,
      cargoManifestFound: true,
      rustSourceReady: true,
      rustSourceReadyRequired: false,
      npmSurfaceReady: true
    });
    assert.deepEqual(report.readinessSummary, {
      readyForReadOnlyOps: true,
      readyForRuntimeOps: false
    });
    assert.deepEqual(report.checks, [
      'vendored binary version',
      'vendored binary help',
      'vendored binary verify doctor',
      'vendored binary verify readiness'
    ]);
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test('verify-vendored-binary can write a fresh report for publish-decision gates', () => {
  if (process.platform === 'win32') {
    return;
  }

  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-verify-vendor-write-'));
  const binaryPath = createVendoredBinaryFixture(path.join(tmp, 'mcpace-fixture'));
  const reportPath = path.join(tmp, 'vendored-binary-latest.json');

  try {
    const result = spawnSync(
      process.execPath,
      [
        verifyScript,
        '--json',
        '--binary-path',
        binaryPath,
        '--target-key',
        'linux-x64-gnu',
        '--write',
        reportPath,
      ],
      {
        cwd: repoRoot,
        encoding: 'utf8',
        env: cleanChildEnv()
      }
    );

    assert.equal(result.status, 0, result.stderr || result.stdout);
    const stdoutReport = JSON.parse(result.stdout);
    const writtenReport = JSON.parse(fs.readFileSync(reportPath, 'utf8'));
    assert.equal(stdoutReport.status, 'pass');
    assert.equal(writtenReport.status, 'pass');
    assert.equal(writtenReport.schema, 'mcpace.vendoredBinary.v1');
    assert.match(writtenReport.generatedAt, /^\d{4}-\d{2}-\d{2}T/);
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test('verify-vendored-binary does not require a Rust toolchain on binary install hosts', () => {
  if (process.platform === 'win32') {
    return;
  }

  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-verify-vendor-runtime-host-'));
  const binaryPath = createVendoredBinaryFixture(path.join(tmp, 'mcpace-fixture'), packageVersion(), {
    rustSourceReady: false,
    cargoManifestFound: true,
  });

  try {
    const result = spawnSync(
      process.execPath,
      [verifyScript, '--json', '--binary-path', binaryPath, '--target-key', 'linux-x64-gnu'],
      {
        cwd: repoRoot,
        encoding: 'utf8',
        env: cleanChildEnv()
      }
    );

    assert.equal(result.status, 0, result.stderr || result.stdout);
    const report = JSON.parse(result.stdout);
    assert.equal(report.status, 'pass');
    assert.deepEqual(report.doctorSummary, {
      configFound: true,
      cargoManifestFound: true,
      rustSourceReady: false,
      rustSourceReadyRequired: false,
      npmSurfaceReady: true
    });
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test('verify-vendored-binary fails clearly when the binary is missing', () => {
  const result = spawnSync(
    process.execPath,
    [verifyScript, '--json', '--binary-path', path.join(repoRoot, 'missing-mcpace')],
    {
      cwd: repoRoot,
      encoding: 'utf8',
      env: cleanChildEnv()
    }
  );

  assert.notEqual(result.status, 0);
  const report = JSON.parse(result.stdout);
  assert.equal(report.status, 'fail');
  assert.match(report.reason, /does not exist/);
});

test('verify-vendored-binary fails clearly when the binary version drifts from the package version', () => {
  if (process.platform === 'win32') {
    return;
  }

  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-verify-vendor-mismatch-'));
  const binaryPath = createVendoredBinaryFixture(path.join(tmp, 'mcpace-fixture'), '9.9.9');

  try {
    const result = spawnSync(
      process.execPath,
      [verifyScript, '--json', '--binary-path', binaryPath, '--target-key', 'linux-x64-gnu'],
      {
        cwd: repoRoot,
        encoding: 'utf8',
        env: cleanChildEnv()
      }
    );

    assert.notEqual(result.status, 0);
    const report = JSON.parse(result.stdout);
    assert.equal(report.status, 'fail');
    assert.match(report.reason, /version mismatch/);
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});
