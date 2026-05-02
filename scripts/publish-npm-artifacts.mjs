#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import { deriveProjectVersion, repoRoot } from './lib/project-metadata.mjs';
import { PLATFORM_PACKAGE_TARGETS } from './lib/npm-platform-packages.mjs';
import { cleanChildEnv } from './lib/safe-child-env.mjs';

const DEFAULT_ARTIFACT_DIR = path.join(repoRoot, 'dist', 'npm');


function parseArgs(argv) {
  const parsed = { json: false, dryRun: false, artifactDir: DEFAULT_ARTIFACT_DIR, skipExisting: true };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--json': parsed.json = true; break;
      case '--dry-run': parsed.dryRun = true; break;
      case '--artifact-dir': parsed.artifactDir = path.resolve(argv[++index] || ''); break;
      case '--no-skip-existing': parsed.skipExisting = false; break;
      default: throw new Error(`unsupported publish-npm-artifacts argument: ${token}`);
    }
  }
  return parsed;
}

function tarballNameForPackage(packageName, version) {
  return `${packageName.replace(/^@/, '').replace('/', '-')}-${version}.tgz`;
}

export function npmPublishChildEnv() {
  return cleanChildEnv({
    MCPACE_NPM_EXEC_PACKAGE: process.env.MCPACE_NPM_EXEC_PACKAGE,
    NPM_CONFIG_REGISTRY: process.env.NPM_CONFIG_REGISTRY,
    NPM_CONFIG_USERCONFIG: process.env.NPM_CONFIG_USERCONFIG,
    NODE_AUTH_TOKEN: process.env.NODE_AUTH_TOKEN,
    NPM_TOKEN: process.env.NPM_TOKEN
  });
}

export function buildNpmInvocation(args, options = {}) {
  const platform = options.platform || process.platform;
  const env = options.env || process.env;
  const exactPackage = String(env.MCPACE_NPM_EXEC_PACKAGE || '').trim();
  const effectiveArgs = exactPackage
    ? ['exec', '--yes', `--package=${exactPackage}`, '--', 'npm', ...args]
    : args;
  const displayCommand = exactPackage
    ? ['npm', 'exec', '--yes', `--package=${exactPackage}`, '--', 'npm', ...args].join(' ')
    : ['npm', ...args].join(' ');

  if (platform === 'win32') {
    return { command: 'cmd.exe', args: ['/d', '/s', '/c', 'npm', ...effectiveArgs], displayCommand };
  }
  return { command: 'npm', args: effectiveArgs, displayCommand };
}

function runNpm(args) {
  const env = npmPublishChildEnv();
  const invocation = buildNpmInvocation(args, { env });
  const result = spawnSync(invocation.command, invocation.args, { cwd: repoRoot, encoding: 'utf8', env, timeout: 120000, windowsHide: true });
  return { command: invocation.displayCommand, status: result.status, ok: result.status === 0, stdout: result.stdout || '', stderr: result.stderr || '', error: result.error ? String(result.error.message || result.error) : null };
}

function summarizeFailure(result) {
  return result.error || [result.stderr, result.stdout].filter(Boolean).join('\n').trim() || `exit code ${result.status}`;
}

function packageVersionExists(packageName, version) {
  const result = runNpm(['view', `${packageName}@${version}`, 'version', '--json']);
  if (result.ok) return { status: 'pass', exists: true, command: result.command };
  const output = [result.stderr, result.stdout, result.error].filter(Boolean).join('\n');
  if (/E404|404 Not Found|not in this registry/i.test(output)) return { status: 'pass', exists: false, command: result.command };
  return { status: 'fail', exists: false, command: result.command, reason: summarizeFailure(result) };
}

function runPublish(tarballPath, dryRun) {
  const args = ['publish', tarballPath, '--access', 'public'];
  if (dryRun) args.push('--dry-run');
  return runNpm(args);
}

export function publishNpmArtifacts(options = {}) {
  const version = deriveProjectVersion();
  const artifactDir = path.resolve(options.artifactDir || DEFAULT_ARTIFACT_DIR);
  const packageNames = [...PLATFORM_PACKAGE_TARGETS.map((target) => target.packageName), '@mcpace/cli'];
  const reports = [];

  for (const packageName of packageNames) {
    const tarballPath = path.join(artifactDir, tarballNameForPackage(packageName, version));
    if (!fs.existsSync(tarballPath)) {
      return { version, status: 'fail', dryRun: Boolean(options.dryRun), artifactDir, published: reports, reason: `missing npm artifact for ${packageName}: ${tarballPath}` };
    }

    if (options.skipExisting !== false && !options.dryRun) {
      const existing = packageVersionExists(packageName, version);
      if (existing.status !== 'pass') return { version, status: 'fail', dryRun: Boolean(options.dryRun), artifactDir, published: reports, reason: `${packageName} existing-version check failed: ${existing.reason}` };
      if (existing.exists) {
        reports.push({ packageName, tarballPath, dryRun: Boolean(options.dryRun), status: 'skip', reason: `${packageName}@${version} already exists`, command: existing.command });
        continue;
      }
    }

    const result = runPublish(tarballPath, Boolean(options.dryRun));
    const report = { packageName, tarballPath, dryRun: Boolean(options.dryRun), status: result.ok ? 'pass' : 'fail', command: result.command };
    reports.push(report);
    if (!result.ok) return { version, status: 'fail', dryRun: Boolean(options.dryRun), artifactDir, published: reports, reason: `${packageName} publish failed: ${summarizeFailure(result)}` };
  }

  return { version, status: 'pass', dryRun: Boolean(options.dryRun), artifactDir, published: reports };
}

function isCliInvocation() {
  const entry = process.argv[1];
  return entry ? pathToFileURL(path.resolve(entry)).href === import.meta.url : false;
}

function main() {
  try {
    const parsed = parseArgs(process.argv.slice(2));
    const report = publishNpmArtifacts(parsed);
    if (parsed.json) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
    else if (report.status === 'pass') {
      const published = report.published.filter((entry) => entry.status === 'pass').length;
      const skipped = report.published.filter((entry) => entry.status === 'skip').length;
      process.stdout.write(`${parsed.dryRun ? 'dry-run ' : ''}published ${published} npm artifacts${skipped ? `, skipped ${skipped} existing` : ''}\n`);
    } else process.stderr.write(`${report.reason}\n`);
    if (report.status !== 'pass') process.exit(1);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exit(1);
  }
}

if (isCliInvocation()) main();
