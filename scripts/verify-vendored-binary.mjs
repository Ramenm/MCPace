#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import {
  SUPPORTED_TARGETS,
  binaryNameForTarget,
  currentTargetKey,
  detectTarget
} from '../packages/npm/cli/lib/platform.js';
import { deriveProjectVersion, repoRoot } from './lib/project-metadata.mjs';

const DEFAULT_BINARY_CHECK_TIMEOUT_MS = 15000;
const BINARY_CHECK_TIMEOUT_MS = parseTimeoutEnv(
  'MCPACE_BINARY_CHECK_TIMEOUT_MS',
  DEFAULT_BINARY_CHECK_TIMEOUT_MS
);

function parseTimeoutEnv(name, fallback) {
  const parsed = Number.parseInt(process.env[name] || '', 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}

function firstNonEmptyLine(value) {
  return String(value || '')
    .split(/\r?\n/)
    .map((line) => line.trim())
    .find(Boolean) || null;
}

function summarizeOutput(stdout = '', stderr = '') {
  const combined = [stdout, stderr].filter(Boolean).join('\n').trim();
  if (!combined) {
    return null;
  }
  return combined.split(/\r?\n/).slice(0, 12).join('\n');
}

function normalizeReportPath(filePath) {
  const absolute = path.resolve(filePath);
  const relative = path.relative(repoRoot, absolute);
  return relative && !relative.startsWith('..') ? relative.split(path.sep).join('/') : absolute;
}

function isExecutable(filePath) {
  if (process.platform === 'win32') {
    return true;
  }

  try {
    fs.accessSync(filePath, fs.constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

function runBinaryCheck(binaryPath, args, label) {
  const startedAt = Date.now();
  const result = spawnSync(binaryPath, args, {
    cwd: repoRoot,
    encoding: 'utf8',
    timeout: BINARY_CHECK_TIMEOUT_MS,
    windowsHide: true
  });

  return {
    label,
    command: `${normalizeReportPath(binaryPath)} ${args.join(' ')}`.trim(),
    ok: result.status === 0,
    status: result.status,
    signal: result.signal ?? null,
    durationMs: Date.now() - startedAt,
    timeoutMs: BINARY_CHECK_TIMEOUT_MS,
    timedOut: result.error?.code === 'ETIMEDOUT',
    stdout: result.stdout || '',
    stderr: result.stderr || '',
    error: result.error ? String(result.error.message || result.error) : null
  };
}

function summarizeFailure(result) {
  if (result.timedOut) {
    return `${result.command} timed out after ${result.timeoutMs}ms`;
  }
  if (result.error) {
    return result.error;
  }

  const combined = summarizeOutput(result.stdout, result.stderr);
  if (combined) {
    return combined;
  }
  return `exit code ${result.status ?? 'unknown'}`;
}

function extractSemver(value) {
  const match = String(value || '').match(/\b(\d+\.\d+\.\d+)\b/);
  return match ? match[1] : null;
}

function parseJsonObject(text, label) {
  try {
    const parsed = JSON.parse(text);
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
      throw new Error(`${label} must be a JSON object`);
    }
    return parsed;
  } catch (error) {
    throw new Error(`${label} output is not valid JSON: ${error instanceof Error ? error.message : String(error)}`);
  }
}

function findTargetByKey(targetKey) {
  return SUPPORTED_TARGETS.find((target) => target.key === targetKey) ?? null;
}

export function resolveVendoredBinary(options = {}) {
  const explicitBinaryPath = options.binaryPath ? path.resolve(options.binaryPath) : null;
  const detectedTarget = options.targetKey ? findTargetByKey(options.targetKey) : detectTarget();
  const resolvedTargetKey = options.targetKey || detectedTarget?.key || currentTargetKey();
  const binaryName = options.binaryName || binaryNameForTarget(
    detectedTarget || { platform: resolvedTargetKey.startsWith('win32-') ? 'win32' : process.platform }
  );
  const binaryPath = explicitBinaryPath || path.join(repoRoot, 'packages', 'npm', 'cli', 'vendor', resolvedTargetKey, binaryName);

  return {
    targetKey: resolvedTargetKey,
    binaryName,
    binaryPath
  };
}

export function parseArgs(argv) {
  const parsed = {
    json: false,
    binaryPath: null,
    targetKey: null,
    expectedVersion: null
  };

  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--json':
        parsed.json = true;
        break;
      case '--binary-path':
        parsed.binaryPath = path.resolve(argv[++index] || '');
        break;
      case '--target-key':
        parsed.targetKey = argv[++index] || null;
        break;
      case '--expected-version':
        parsed.expectedVersion = argv[++index] || null;
        break;
      default:
        throw new Error(`unsupported verify-vendored-binary argument: ${token}`);
    }
  }

  return parsed;
}

export function verifyVendoredBinary(options = {}) {
  const resolved = resolveVendoredBinary(options);
  const expectedVersion = options.expectedVersion || deriveProjectVersion();
  const report = {
    status: 'fail',
    targetKey: resolved.targetKey,
    binaryName: resolved.binaryName,
    binaryPath: normalizeReportPath(resolved.binaryPath),
    expectedVersion,
    checks: []
  };

  if (!fs.existsSync(resolved.binaryPath)) {
    report.reason = `vendored binary does not exist: ${normalizeReportPath(resolved.binaryPath)}`;
    return report;
  }

  let stat;
  try {
    stat = fs.statSync(resolved.binaryPath);
  } catch (error) {
    report.reason = `failed to stat vendored binary: ${error instanceof Error ? error.message : String(error)}`;
    return report;
  }

  if (!stat.isFile()) {
    report.reason = `vendored binary is not a file: ${normalizeReportPath(resolved.binaryPath)}`;
    return report;
  }

  if (!isExecutable(resolved.binaryPath)) {
    report.reason = `vendored binary is not executable: ${normalizeReportPath(resolved.binaryPath)}`;
    return report;
  }

  const versionCheck = runBinaryCheck(resolved.binaryPath, ['version'], 'vendored binary version');
  if (!versionCheck.ok) {
    report.reason = `vendored binary version check failed: ${summarizeFailure(versionCheck)}`;
    return report;
  }

  const versionOutput = firstNonEmptyLine(versionCheck.stdout) || firstNonEmptyLine(versionCheck.stderr);
  if (!versionOutput) {
    report.reason = 'vendored binary version check produced no output';
    return report;
  }

  const binaryVersion = extractSemver(versionOutput);
  if (!binaryVersion || binaryVersion !== expectedVersion) {
    report.reason = `vendored binary version mismatch: expected ${expectedVersion}, got ${versionOutput}`;
    return report;
  }

  report.checks.push(versionCheck.label);
  report.versionOutput = versionOutput;
  report.binaryVersion = binaryVersion;
  report.versionCommand = versionCheck.command;

  const helpCheck = runBinaryCheck(resolved.binaryPath, ['help'], 'vendored binary help');
  if (!helpCheck.ok) {
    report.reason = `vendored binary help check failed: ${summarizeFailure(helpCheck)}`;
    return report;
  }

  const helpText = [helpCheck.stdout, helpCheck.stderr].filter(Boolean).join('\n').trim();
  if (!helpText) {
    report.reason = 'vendored binary help check produced no output';
    return report;
  }

  report.checks.push(helpCheck.label);
  report.helpMentionsMcpace = /mcpace/i.test(helpText);
  report.helpCommand = helpCheck.command;
  report.helpSample = summarizeOutput(helpCheck.stdout, helpCheck.stderr);
  if (!report.helpMentionsMcpace) {
    report.reason = 'vendored binary help output does not mention mcpace';
    return report;
  }

  const doctorCheck = runBinaryCheck(
    resolved.binaryPath,
    ['verify', 'doctor', '--json'],
    'vendored binary verify doctor'
  );
  if (!doctorCheck.ok) {
    report.reason = `vendored binary verify doctor check failed: ${summarizeFailure(doctorCheck)}`;
    return report;
  }

  const doctorText = [doctorCheck.stdout, doctorCheck.stderr].find((value) => String(value || '').trim());
  if (!doctorText) {
    report.reason = 'vendored binary verify doctor check produced no output';
    return report;
  }

  let doctorJson;
  try {
    doctorJson = parseJsonObject(doctorText, 'vendored binary verify doctor');
  } catch (error) {
    report.reason = error instanceof Error ? error.message : String(error);
    return report;
  }

  if (doctorJson.configFound !== true || doctorJson.rustSourceReady !== true || doctorJson.npmSurfaceReady !== true) {
    report.reason = 'vendored binary verify doctor output does not confirm config/rust/npm readiness';
    return report;
  }

  report.checks.push(doctorCheck.label);
  report.doctorCommand = doctorCheck.command;
  report.doctorSummary = {
    configFound: doctorJson.configFound,
    rustSourceReady: doctorJson.rustSourceReady,
    npmSurfaceReady: doctorJson.npmSurfaceReady
  };

  const readinessCheck = runBinaryCheck(
    resolved.binaryPath,
    ['verify', 'readiness', '--json'],
    'vendored binary verify readiness'
  );
  if (!readinessCheck.ok) {
    report.reason = `vendored binary verify readiness check failed: ${summarizeFailure(readinessCheck)}`;
    return report;
  }

  const readinessText = [readinessCheck.stdout, readinessCheck.stderr].find((value) => String(value || '').trim());
  if (!readinessText) {
    report.reason = 'vendored binary verify readiness check produced no output';
    return report;
  }

  let readinessJson;
  try {
    readinessJson = parseJsonObject(readinessText, 'vendored binary verify readiness');
  } catch (error) {
    report.reason = error instanceof Error ? error.message : String(error);
    return report;
  }

  if (readinessJson.readyForReadOnlyOps !== true || typeof readinessJson.readyForRuntimeOps !== 'boolean') {
    report.reason = 'vendored binary verify readiness output does not confirm read-only readiness';
    return report;
  }

  report.checks.push(readinessCheck.label);
  report.readinessCommand = readinessCheck.command;
  report.readinessSummary = {
    readyForReadOnlyOps: readinessJson.readyForReadOnlyOps,
    readyForRuntimeOps: readinessJson.readyForRuntimeOps
  };

  report.status = 'pass';
  return report;
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
    const report = verifyVendoredBinary(parsed);
    if (parsed.json) {
      process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
    } else if (report.status === 'pass') {
      process.stdout.write(`${report.binaryPath}\n`);
    } else {
      process.stderr.write(`${report.reason}\n`);
    }

    if (report.status !== 'pass') {
      process.exit(1);
    }
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exit(1);
  }
}

if (isCliInvocation()) {
  main();
}
