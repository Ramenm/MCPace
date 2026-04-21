import fs from 'node:fs';
import { execFileSync } from 'node:child_process';

export const SUPPORTED_TARGETS = [
  { key: 'darwin-x64', platform: 'darwin', arch: 'x64', triple: 'x86_64-apple-darwin' },
  { key: 'darwin-arm64', platform: 'darwin', arch: 'arm64', triple: 'aarch64-apple-darwin' },
  { key: 'linux-x64-gnu', platform: 'linux', arch: 'x64', libc: 'gnu', triple: 'x86_64-unknown-linux-gnu' },
  { key: 'linux-arm64-gnu', platform: 'linux', arch: 'arm64', libc: 'gnu', triple: 'aarch64-unknown-linux-gnu' },
  { key: 'win32-x64-msvc', platform: 'win32', arch: 'x64', triple: 'x86_64-pc-windows-msvc' }
];

export function detectLibc(platform = process.platform) {
  if (platform !== 'linux') {
    return null;
  }

  const report = process.report?.getReport?.();
  if (report?.header?.glibcVersionRuntime) {
    return 'gnu';
  }
  if (Array.isArray(report?.sharedObjects) && report.sharedObjects.some((entry) => /musl/i.test(String(entry)))) {
    return 'musl';
  }

  try {
    const output = execFileSync('ldd', ['--version'], { encoding: 'utf8' });
    if (/musl/i.test(output)) {
      return 'musl';
    }
    if (/glibc|gnu/i.test(output)) {
      return 'gnu';
    }
  } catch {
    // fall through to the filesystem heuristic below
  }

  try {
    if (fs.existsSync('/etc/alpine-release')) {
      return 'musl';
    }
  } catch {
    // ignore filesystem probing errors and report the libc as unknown
  }

  return null;
}

export function currentTargetKey(
  platform = process.platform,
  arch = process.arch,
  libc = platform === 'linux' ? detectLibc(platform) : null
) {
  return [platform, arch, libc].filter(Boolean).join('-');
}

export function detectTarget(
  platform = process.platform,
  arch = process.arch,
  libc = platform === 'linux' ? detectLibc(platform) : null
) {
  return SUPPORTED_TARGETS.find((entry) => entry.platform === platform && entry.arch === arch && (!entry.libc || entry.libc === libc)) ?? null;
}

export function packageNamesForTarget(target) {
  if (!target) {
    return [];
  }
  return [`@mcpace/cli-${target.key}`];
}

export function describeSupportedTargets() {
  return SUPPORTED_TARGETS.map((entry) => entry.key).join(', ');
}
