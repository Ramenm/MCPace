#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { repoRoot } from './lib/project-metadata.mjs';

const DARWIN_TARGETS = [
  {
    key: 'darwin-x64',
    rustTarget: 'x86_64-apple-darwin',
    runner: 'macos-15-intel',
    packageName: '@mcpace/cli-darwin-x64',
    cpu: 'x64'
  },
  {
    key: 'darwin-arm64',
    rustTarget: 'aarch64-apple-darwin',
    runner: 'macos-15',
    packageName: '@mcpace/cli-darwin-arm64',
    cpu: 'arm64'
  }
];

function parseArgs(argv) {
  const parsed = { json: false, cargoCheck: false };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--json':
        parsed.json = true;
        break;
      case '--cargo-check':
        parsed.cargoCheck = true;
        break;
      default:
        throw new Error(`unsupported verify-macos-proof-lanes argument: ${token}`);
    }
  }
  return parsed;
}

function readJson(relativePath) {
  return JSON.parse(fs.readFileSync(path.join(repoRoot, relativePath), 'utf8'));
}

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

function pushIssue(issues, condition, message) {
  if (!condition) {
    issues.push(message);
  }
}

function verifyReleaseTargets(issues) {
  const manifest = readJson('release-targets.json');
  const targets = new Map((manifest.targets || []).map((target) => [target.key, target]));
  for (const expected of DARWIN_TARGETS) {
    const target = targets.get(expected.key);
    pushIssue(issues, !!target, `release-targets.json is missing ${expected.key}`);
    if (!target) continue;
    pushIssue(issues, target.platform === 'darwin', `${expected.key} platform must be darwin`);
    pushIssue(issues, target.rustTarget === expected.rustTarget, `${expected.key} rustTarget drifted`);
    pushIssue(issues, target.runner === expected.runner, `${expected.key} runner must be ${expected.runner}`);
    pushIssue(issues, target.packageName === expected.packageName, `${expected.key} package name drifted`);
    pushIssue(issues, target.binaryName === 'mcpace', `${expected.key} binaryName must be mcpace`);
    pushIssue(issues, target.publishEnabled === true, `${expected.key} must be publish enabled`);
  }
}

function verifyPlatformPackages(issues) {
  for (const expected of DARWIN_TARGETS) {
    const manifest = readJson(path.join('packages', 'npm', `cli-${expected.key}`, 'package.json'));
    pushIssue(issues, manifest.name === expected.packageName, `${expected.key} npm manifest name drifted`);
    pushIssue(issues, Array.isArray(manifest.os) && manifest.os.length === 1 && manifest.os[0] === 'darwin', `${expected.key} npm os filter must be darwin`);
    pushIssue(issues, Array.isArray(manifest.cpu) && manifest.cpu.length === 1 && manifest.cpu[0] === expected.cpu, `${expected.key} npm cpu filter drifted`);
    pushIssue(
      issues,
      Array.isArray(manifest.files) && (manifest.files.includes('bin') || manifest.files.includes('bin/')),
      `${expected.key} npm files must include bin`
    );
  }
}

function verifyWorkflows(issues) {
  const ci = read(path.join('.github', 'workflows', 'ci.yml'));
  const release = read(path.join('.github', 'workflows', 'release.yml'));
  const dryRun = read(path.join('.github', 'workflows', 'release-dry-run.yml'));
  pushIssue(issues, /macos-latest/.test(ci), 'ci.yml must keep macos-latest validation lanes');
  pushIssue(issues, /launcher-fast-hosted:[\s\S]*macos-latest/.test(ci), 'ci.yml must keep macOS hosted launcher smoke');
  pushIssue(issues, /rust-lifecycle-validation:[\s\S]*macos-latest/.test(ci), 'ci.yml must keep macOS lifecycle validation');
  pushIssue(issues, /macos-15/.test(release), 'release.yml must keep explicit macOS lifecycle lanes');
  pushIssue(issues, /macos-15/.test(dryRun), 'release-dry-run.yml must keep explicit macOS lifecycle lanes');
  pushIssue(issues, /node scripts\/github-release-matrix\.mjs --github-output/.test(release), 'release.yml must use generated matrix from release-targets.json');
  pushIssue(issues, /node scripts\/github-release-matrix\.mjs --github-output/.test(dryRun), 'release-dry-run.yml must use generated matrix from release-targets.json');
}

function verifyAutostartSource(issues) {
  const service = read(path.join('src', 'service.rs'));
  pushIssue(issues, /MacOSLaunchMode::LaunchAgent/.test(service), 'service.rs must use macOS LaunchAgent mode');
  pushIssue(issues, /auto-launch\/macos-launch-agent/.test(service), 'service.rs must report macOS LaunchAgent backend');
  pushIssue(issues, /KeepAlive<\/key><false\/>/.test(service), 'macOS LaunchAgent should not KeepAlive by default');
  pushIssue(issues, /LinuxLaunchMode::Systemd/.test(service), 'service.rs must keep Linux systemd user mode separate from macOS LaunchAgent');
}

function runCargoCheck(target, issues) {
  const targetAdd = spawnSync('rustup', ['+stable', 'target', 'add', target], {
    cwd: repoRoot,
    encoding: 'utf8',
    windowsHide: true,
    timeout: 300000
  });
  if (targetAdd.status !== 0) {
    issues.push(`rustup target add ${target} failed: ${(targetAdd.stderr || targetAdd.stdout || '').trim()}`);
    return { target, status: targetAdd.status, phase: 'rustup-target-add' };
  }

  const check = spawnSync('cargo', ['+stable', 'check', '--target', target], {
    cwd: repoRoot,
    encoding: 'utf8',
    windowsHide: true,
    timeout: 600000
  });
  if (check.status !== 0) {
    issues.push(`cargo check --target ${target} failed: ${(check.stderr || check.stdout || '').trim()}`);
  }
  return { target, status: check.status, phase: 'cargo-check' };
}

function main() {
  const parsed = parseArgs(process.argv.slice(2));
  const issues = [];
  verifyReleaseTargets(issues);
  verifyPlatformPackages(issues);
  verifyWorkflows(issues);
  verifyAutostartSource(issues);

  const cargoChecks = parsed.cargoCheck
    ? DARWIN_TARGETS.map((target) => runCargoCheck(target.rustTarget, issues))
    : [];

  const report = {
    status: issues.length === 0 ? 'pass' : 'fail',
    mode: parsed.cargoCheck ? 'contracts-plus-cross-cargo-check' : 'contracts-only',
    darwinTargets: DARWIN_TARGETS,
    cargoChecks,
    issues
  };

  if (parsed.json) {
    process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
  } else if (report.status === 'pass') {
    process.stdout.write(`macOS proof-lane verification passed (${report.mode})\n`);
  } else {
    process.stderr.write(`${JSON.stringify(report, null, 2)}\n`);
  }

  if (report.status !== 'pass') {
    process.exit(1);
  }
}

main();
