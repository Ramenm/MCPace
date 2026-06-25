import fs from 'node:fs';
import { execFileSync } from 'node:child_process';
import { SUPPORTED_TARGETS as RELEASE_SUPPORTED_TARGETS } from './targets.js';

const DEFAULT_PLATFORM_PROBE_TIMEOUT_MS = 3000;
const PLATFORM_PROBE_TIMEOUT_MS = parseTimeoutEnv(
  'MCPACE_PLATFORM_PROBE_TIMEOUT_MS',
  DEFAULT_PLATFORM_PROBE_TIMEOUT_MS
);
const libcProbeCache = new Map();
const TRUSTED_LDD_PATHS = ['/usr/bin/ldd', '/bin/ldd'];
const PLATFORM_PROBE_ENV = Object.freeze({
  PATH: '/usr/sbin:/usr/bin:/sbin:/bin',
  LC_ALL: 'C',
});

function parseTimeoutEnv(name, fallback) {
  const parsed = Number.parseInt(process.env[name] || '', 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}

export const SUPPORTED_TARGETS = Object.freeze(
  RELEASE_SUPPORTED_TARGETS.map((target) => Object.freeze({ ...target }))
);

export function binaryNameForPlatform(platform = process.platform) {
  return platform === 'win32' ? 'mcpace.exe' : 'mcpace';
}

export function binaryNameForTarget(target) {
  return target?.binaryName ?? binaryNameForPlatform(target?.platform ?? process.platform);
}

function isTrustedProbeExecutable(filePath) {
  try {
    const stat = fs.lstatSync(filePath);
    return stat.isFile() && (process.platform === 'win32' || (Number(stat.mode) & 0o111) !== 0);
  } catch {
    return false;
  }
}

function detectLibcWithTrustedLdd() {
  for (const candidate of TRUSTED_LDD_PATHS) {
    if (!isTrustedProbeExecutable(candidate)) {
      continue;
    }
    try {
      const output = execFileSync(candidate, ['--version'], {
        encoding: 'utf8',
        env: PLATFORM_PROBE_ENV,
        timeout: PLATFORM_PROBE_TIMEOUT_MS,
        windowsHide: true
      });
      if (/musl/i.test(output)) {
        return 'musl';
      }
      if (/glibc|gnu/i.test(output)) {
        return 'gnu';
      }
    } catch {
      // Try the next trusted absolute probe path, then fall through to filesystem heuristics.
    }
  }
  return null;
}

export function detectLibc(platform = process.platform) {
  if (platform !== 'linux') {
    return null;
  }

  if (libcProbeCache.has(platform)) {
    return libcProbeCache.get(platform);
  }

  let detected = null;
  const report = process.report?.getReport?.();
  if (report?.header?.glibcVersionRuntime) {
    detected = 'gnu';
  } else if (Array.isArray(report?.sharedObjects) && report.sharedObjects.some((entry) => /musl/i.test(String(entry)))) {
    detected = 'musl';
  } else {
    detected = detectLibcWithTrustedLdd();
  }

  if (!detected) {
    try {
      if (fs.existsSync('/etc/alpine-release')) {
        detected = 'musl';
      }
    } catch {
      // ignore filesystem probing errors and report the libc as unknown
    }
  }

  libcProbeCache.set(platform, detected);
  return detected;
}

export function currentTargetKey(
  platform = process.platform,
  arch = process.arch,
  libc = platform === 'linux' ? detectLibc(platform) : null
) {
  return [platform, arch, libc].filter(Boolean).join('-');
}

function targetLibcProbe(target) {
  if (!target) {
    return null;
  }
  if (target.libcProbe) {
    return target.libcProbe;
  }
  if (Array.isArray(target.libc)) {
    return target.libc.includes('musl') ? 'musl' : 'gnu';
  }
  return target.libc || null;
}

export function detectTarget(
  platform = process.platform,
  arch = process.arch,
  libc = platform === 'linux' ? detectLibc(platform) : null
) {
  return SUPPORTED_TARGETS.find((entry) => {
    if (entry.platform !== platform || entry.arch !== arch) {
      return false;
    }
    const expectedLibc = targetLibcProbe(entry);
    return !expectedLibc || expectedLibc === libc;
  }) ?? null;
}

export function packageNamesForTarget(target) {
  if (!target) {
    return [];
  }
  return [target.packageName ?? target.npmPackage ?? `@mcpace/cli-${target.key}`];
}

export function describeSupportedTargets() {
  return SUPPORTED_TARGETS.map((entry) => entry.key).join(', ');
}
