#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import { SUPPORTED_TARGETS, binaryNameForTarget, currentTargetKey, detectTarget } from '../packages/npm/cli/lib/platform.js';
import {
  deriveProjectVersion,
  readJson,
  repoRoot
} from './lib/project-metadata.mjs';
import { readClientCatalog, resolveInstallSupportTargets, resolveProofFocusTargets } from './lib/client-catalog.mjs';
import { verifyNpmPack } from './verify-npm-pack.mjs';
import { verifyVendoredBinary } from './verify-vendored-binary.mjs';
const DEFAULT_OUTPUT_PATH = path.join(repoRoot, 'reports', 'verification-latest.json');
const DEFAULT_ARCHIVE_OUTPUT_DIR = path.join(repoRoot, 'dist');
const DEFAULT_VERSION_PROBE_TIMEOUT_MS = 3000;
const DEFAULT_PROOF_COMMAND_TIMEOUT_MS = 300000;
const VERSION_PROBE_TIMEOUT_MS = parseTimeoutEnv(
  'MCPACE_VERSION_PROBE_TIMEOUT_MS',
  DEFAULT_VERSION_PROBE_TIMEOUT_MS
);
const PROOF_COMMAND_TIMEOUT_MS = parseTimeoutEnv(
  'MCPACE_PROOF_COMMAND_TIMEOUT_MS',
  DEFAULT_PROOF_COMMAND_TIMEOUT_MS
);
const IMPLEMENTATION_STATUS_ORDER = ['implemented', 'planned', 'missing'];
const CLAIM_STATUS_ORDER = [
  'supported',
  'supported-local-only',
  'control-plane-only',
  'bootstrap-only',
  'connectable-preview',
  'requires-host-proof',
  'planned'
];

function parseTimeoutEnv(name, fallback) {
  const parsed = Number.parseInt(process.env[name] || '', 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}

function childEnvForCommand(command) {
  const env = { ...process.env };
  delete env.NODE_TEST_CONTEXT;

  if ((command === 'cargo' || command === 'rustc') && !env.RUSTUP_TOOLCHAIN) {
    env.RUSTUP_TOOLCHAIN = 'stable';
  }

  return env;
}

export function resolveCommandInvocation(command, args = [], platform = process.platform) {
  if (platform === 'win32' && command === 'npm') {
    return {
      bin: 'cmd.exe',
      args: ['/d', '/s', '/c', 'npm', ...args],
      displayCommand: ['npm', ...args].join(' ')
    };
  }

  return {
    bin: command,
    args,
    displayCommand: [command, ...args].join(' ')
  };
}

function firstNonEmptyLine(value) {
  return String(value || '')
    .split(/\r?\n/)
    .map((line) => line.trim())
    .find(Boolean) || null;
}

function normalizePathForReport(filePath) {
  const relative = path.relative(repoRoot, filePath);
  return relative && !relative.startsWith('..') ? relative.split(path.sep).join('/') : filePath;
}

function detectContainerEnvironment() {
  return Boolean(
    process.env.CONTAINER === 'true' ||
      fs.existsSync('/.dockerenv') ||
      fs.existsSync('/run/.containerenv')
  );
}

function detectCommandVersion(command, args = ['--version']) {
  const invocation = resolveCommandInvocation(command, args);
  const result = spawnSync(invocation.bin, invocation.args, {
    cwd: repoRoot,
    encoding: 'utf8',
    env: childEnvForCommand(command),
    timeout: VERSION_PROBE_TIMEOUT_MS,
    windowsHide: true
  });

  if (result.error || result.status !== 0) {
    return null;
  }

  return firstNonEmptyLine(result.stdout) || firstNonEmptyLine(result.stderr);
}

function parseMajorVersion(value) {
  const match = String(value || '').trim().match(/^(?:v)?(\d+)/);
  return match ? Number(match[1]) : Number.NaN;
}

function parseMinimumMajorFromRange(range, fallback) {
  const match = String(range || '').match(/>=\s*(\d+)/);
  return match ? Number(match[1]) : fallback;
}

function summarizeContributorToolchainPolicy(environment) {
  const packageJson = readJson('package.json');
  const requiredNodeMajor = parseMinimumMajorFromRange(packageJson.engines?.node, 22);
  const requiredNpmMajor = parseMinimumMajorFromRange(packageJson.engines?.npm, 10);
  const currentNodeMajor = parseMajorVersion(environment.node);
  const currentNpmMajor = parseMajorVersion(environment.npm);
  const supportedNode = Number.isInteger(currentNodeMajor) && currentNodeMajor >= requiredNodeMajor;
  const supportedNpm = Number.isInteger(currentNpmMajor) && currentNpmMajor >= requiredNpmMajor;

  return {
    requiredNodeMajor,
    requiredNpmMajor,
    currentNodeMajor: Number.isInteger(currentNodeMajor) ? currentNodeMajor : null,
    currentNpmMajor: Number.isInteger(currentNpmMajor) ? currentNpmMajor : null,
    supportedNode,
    supportedNpm,
    supportedContributorToolchain: supportedNode && supportedNpm
  };
}

function contributorToolchainReason(policy, environment) {
  return (
    `source checks ran under unsupported contributor toolchain: node ${environment.node || 'unknown'}, ` +
    `npm ${environment.npm || 'unknown'}; project policy requires Node ${policy.requiredNodeMajor}+ and npm ${policy.requiredNpmMajor}+.`
  );
}

function readProductTruth(version) {
  const value = readJson('docs/product-truth.json');
  if (value.version !== version) {
    throw new Error(`docs/product-truth.json version ${value.version} does not match project version ${version}`);
  }
  const proofFocusTargets = resolveProofFocusTargets(value);
  const installSupportTargets = resolveInstallSupportTargets(value);
  return {
    ...value,
    proofFocusSurfaces: proofFocusTargets.map((target) => target.id),
    proofFocusSurfaceCount: proofFocusTargets.length,
    installSupportedSurfaces: installSupportTargets.map((target) => target.id)
  };
}

function summarizeClientCatalog(productTruth) {
  const targets = readClientCatalog();
  const proofTierCounts = {};
  const installSupportedSurfaces = [];

  for (const target of targets) {
    proofTierCounts[target.proofTier] = (proofTierCounts[target.proofTier] || 0) + 1;
    if (target.installSupported) {
      installSupportedSurfaces.push(target.id);
    }
  }

  return {
    totalTargets: targets.length,
    proofTierCounts,
    installSupportedCount: installSupportedSurfaces.length,
    installSupportedSurfaces,
    proofFocusSurfaces: resolveProofFocusTargets(productTruth).map((target) => target.id)
  };
}

function summarizeCapabilityInventory(version) {
  const value = readJson('eval/runtime-capabilities.json');
  if (value.version !== version) {
    throw new Error(`eval/runtime-capabilities.json version ${value.version} does not match project version ${version}`);
  }

  const implementationStatusCounts = Object.fromEntries(IMPLEMENTATION_STATUS_ORDER.map((status) => [status, 0]));
  const claimStatusCounts = Object.fromEntries(CLAIM_STATUS_ORDER.map((status) => [status, 0]));

  for (const feature of value.features || []) {
    implementationStatusCounts[feature.status] = (implementationStatusCounts[feature.status] || 0) + 1;
    claimStatusCounts[feature.claimStatus] = (claimStatusCounts[feature.claimStatus] || 0) + 1;
  }

  return {
    totalCapabilities: Array.isArray(value.features) ? value.features.length : 0,
    implementationStatusCounts,
    claimStatusCounts,
    claimStatusLegend: value.claimStatusLegend || null
  };
}

function detectVendoredBinaryTargets() {
  const vendorRoot = path.join(repoRoot, 'packages', 'npm', 'cli', 'vendor');
  return SUPPORTED_TARGETS.filter((target) =>
    fs.existsSync(path.join(vendorRoot, target.key, binaryNameForTarget(target)))
  ).map((target) => target.key);
}

function summarizeFailure(result) {
  if (result.timedOut) {
    return `${result.command} timed out after ${result.timeoutMs}ms`;
  }
  if (result.error) {
    return result.error;
  }

  const combined = [result.stderr, result.stdout]
    .filter(Boolean)
    .join('\n')
    .trim();
  if (!combined) {
    return `exit code ${result.status ?? 'unknown'}`;
  }
  return combined
    .split(/\r?\n/)
    .slice(-12)
    .join('\n');
}

function runCheckedCommand(command, args, label, cwd = repoRoot, timeoutMs = PROOF_COMMAND_TIMEOUT_MS) {
  const startedAt = Date.now();
  const invocation = resolveCommandInvocation(command, args);
  const result = spawnSync(invocation.bin, invocation.args, {
    cwd,
    encoding: 'utf8',
    env: childEnvForCommand(command),
    timeout: timeoutMs,
    windowsHide: true
  });
  return {
    label,
    command: invocation.displayCommand,
    ok: result.status === 0,
    status: result.status,
    signal: result.signal ?? null,
    durationMs: Date.now() - startedAt,
    timeoutMs,
    timedOut: result.error?.code === 'ETIMEDOUT',
    stdout: result.stdout || '',
    stderr: result.stderr || '',
    error: result.error ? String(result.error.message || result.error) : null
  };
}

function runArchiveBuilder(outputDir, stamp = null) {
  const args = ['scripts/archive-release.mjs', '--json', '--output-dir', outputDir];
  if (stamp) {
    args.push('--stamp', stamp);
  }

  const result = runCheckedCommand(
    process.execPath,
    args,
    'node scripts/archive-release.mjs --json'
  );

  if (!result.ok) {
    return result;
  }

  try {
    result.archive = JSON.parse(result.stdout);
  } catch (error) {
    result.ok = false;
    result.error = `failed to parse archive builder output: ${error instanceof Error ? error.message : String(error)}`;
  }

  return result;
}

export function parseArgs(argv) {
  const parsed = {
    json: false,
    write: false,
    noRun: false,
    checkedAt: null,
    outputPath: DEFAULT_OUTPUT_PATH,
    archiveOutputDir: DEFAULT_ARCHIVE_OUTPUT_DIR,
    archiveStamp: null
  };

  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--json':
        parsed.json = true;
        break;
      case '--write':
        parsed.write = true;
        break;
      case '--no-run':
        parsed.noRun = true;
        break;
      case '--checked-at':
        parsed.checkedAt = argv[++index] || null;
        break;
      case '--output-path':
        parsed.outputPath = path.resolve(argv[++index] || '');
        break;
      case '--archive-output-dir':
        parsed.archiveOutputDir = path.resolve(argv[++index] || '');
        break;
      case '--archive-stamp':
        parsed.archiveStamp = argv[++index] || null;
        break;
      default:
        throw new Error(`unsupported proof-report argument: ${token}`);
    }
  }

  return parsed;
}

export function collectReport(options = {}) {
  const checkedAt = options.checkedAt || new Date().toISOString();
  const noRun = Boolean(options.noRun);
  const vendoredBinaryTargets = detectVendoredBinaryTargets();
  const environment = {
    container: detectContainerEnvironment(),
    node: process.version,
    npm: noRun ? null : detectCommandVersion('npm', ['--version']),
    cargo: noRun ? null : detectCommandVersion('cargo', ['--version']),
    rustc: noRun ? null : detectCommandVersion('rustc', ['--version'])
  };
  environment.toolchainPolicy = summarizeContributorToolchainPolicy(environment);

  const detectedCurrentTarget = detectTarget();
  const currentTarget = detectedCurrentTarget?.key || currentTargetKey();
  const currentTargetVendoredBinaryStaged = vendoredBinaryTargets.includes(currentTarget);
  const currentTargetVendoredBinary = currentTargetVendoredBinaryStaged
    ? (options.noRun
        ? {
            status: 'not-run',
            targetKey: currentTarget,
            reason: 'proof commands were skipped via --no-run'
          }
        : verifyVendoredBinary({ targetKey: currentTarget }))
    : null;
  const currentTargetPackagingMode = currentTargetVendoredBinary?.status === 'pass'
    ? 'self-contained-vendored-binary'
    : currentTargetVendoredBinaryStaged
      ? 'vendored-binary-staged-but-unverified'
      : environment.cargo && environment.rustc
        ? 'source-build-required'
        : 'blocked-without-vendored-binary-or-rust-toolchain';

  const distribution = {
    currentTarget,
    currentTargetPackagingMode,
    vendoredBinaryTargets,
    currentTargetVendoredBinary
  };

  const version = deriveProjectVersion();
  const productTruth = readProductTruth(version);
  const clientCatalog = summarizeClientCatalog(productTruth);
  const capabilityInventory = summarizeCapabilityInventory(version);

  let sourceProof;
  let releaseProof;

  if (options.noRun) {
    sourceProof = {
      status: 'not-run',
      checks: [],
      reason: 'proof commands were skipped via --no-run'
    };
    releaseProof = {
      status: 'not-run',
      checks: [],
      reason: 'proof commands were skipped via --no-run'
    };
  } else if (!environment.npm) {
    sourceProof = {
      status: 'blocked',
      checks: [],
      reason: 'npm is not installed in this environment'
    };
    releaseProof = {
      status: 'blocked',
      checks: [],
      reason: 'npm is not installed in this environment'
    };
  } else {
    const source = runCheckedCommand('npm', ['test'], 'npm test');
    if (!source.ok) {
      sourceProof = {
        status: 'fail',
        checks: [],
        reason: source.error || summarizeFailure(source)
      };
      releaseProof = {
        status: 'blocked',
        checks: [],
        reason: 'source proof failed; release proof was not attempted'
      };
    } else {
      sourceProof = {
        status: environment.toolchainPolicy.supportedContributorToolchain ? 'pass' : 'partial',
        checks: [source.label],
        durationMs: source.durationMs,
        ...(environment.toolchainPolicy.supportedContributorToolchain
          ? {}
          : {
              reason: contributorToolchainReason(environment.toolchainPolicy, environment)
            })
      };

      const releaseChecks = [source.label];
      const pack = verifyNpmPack();
      if (pack.status !== 'pass') {
        releaseProof = {
          status: 'fail',
          checks: releaseChecks,
          reason: pack.reason,
          npmPackage: pack
        };
      } else {
        releaseChecks.push('npm pack contract (@mcpace/cli)');
        const archive = runArchiveBuilder(options.archiveOutputDir || DEFAULT_ARCHIVE_OUTPUT_DIR, options.archiveStamp || null);
        if (!archive.ok || !archive.archive) {
          releaseProof = {
            status: 'fail',
            checks: releaseChecks,
            reason: archive.error || summarizeFailure(archive)
          };
        } else {
          releaseChecks.push(archive.label);
          if (currentTargetVendoredBinary?.status === 'pass') {
            releaseChecks.push(`vendored binary smoke (${currentTarget})`);
          }
          releaseProof = {
            status: 'partial',
            checks: releaseChecks,
            archive: {
              name: archive.archive.archiveName,
              path: normalizePathForReport(archive.archive.archivePath),
              stamp: archive.archive.stamp
            },
            npmPackage: pack,
            vendoredBinaryTargets,
            currentTargetPackagingMode,
            currentTargetVendoredBinary,
            missing: [
              'GitHub Release artifact publication',
              'published npm provenance proof',
              'real-host runtime validation before public release claims',
              ...(!environment.toolchainPolicy.supportedContributorToolchain
                ? [contributorToolchainReason(environment.toolchainPolicy, environment)]
                : []),
              ...(vendoredBinaryTargets.length === 0
                ? ['vendored binary bundle for at least one supported target']
                : []),
              ...(currentTargetVendoredBinaryStaged && currentTargetVendoredBinary?.status !== 'pass'
                ? [`smoke-verified vendored binary for current target ${currentTarget}`]
                : [])
            ]
          };
        }
      }
    }
  }

  const buildProof = !environment.cargo || !environment.rustc
    ? {
        status: 'blocked',
        checks: [],
        reason: 'cargo/rustc are not installed in this environment'
      }
    : {
        status: 'not-run',
        checks: [],
        reason: 'Rust host proof was not executed by this report script'
      };

  const runtimeProof = environment.container
    ? {
        status: 'blocked',
        checks: [],
        reason: 'no supported real-host runtime proof was executed in this container'
      }
    : {
        status: 'not-run',
        checks: [],
        reason: 'runtime proof was not executed by this report script'
      };

  return {
    version,
    checkedAt,
    productTruth,
    clientCatalog,
    capabilityInventory,
    environment,
    distribution,
    sourceProof,
    buildProof,
    runtimeProof,
    releaseProof
  };
}

export function writeReport(report, outputPath = DEFAULT_OUTPUT_PATH) {
  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  fs.writeFileSync(outputPath, `${JSON.stringify(report, null, 2)}\n`, 'utf8');
  return outputPath;
}

function isCliInvocation() {
  const entry = process.argv[1];
  if (!entry) {
    return false;
  }
  return pathToFileURL(path.resolve(entry)).href === import.meta.url;
}

function main() {
  try {
    const parsed = parseArgs(process.argv.slice(2));
    const report = collectReport(parsed);
    if (parsed.write) {
      writeReport(report, parsed.outputPath);
      const archiveInfo = report.releaseProof?.archive;
      if (!parsed.noRun && archiveInfo?.stamp) {
        const resyncedArchive = runArchiveBuilder(parsed.archiveOutputDir, archiveInfo.stamp);
        if (!resyncedArchive.ok) {
          throw new Error(resyncedArchive.error || summarizeFailure(resyncedArchive));
        }
      }
    }
    if (parsed.json) {
      process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
      return;
    }
    process.stdout.write(`${parsed.write ? parsed.outputPath : 'proof report generated'}\n`);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exit(1);
  }
}

if (isCliInvocation()) {
  main();
}
