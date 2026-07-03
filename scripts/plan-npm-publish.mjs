#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import process from 'node:process';
import { readCliPackageJson } from './lib/project-metadata.mjs';

function parseArgs(argv) {
  return {
    githubOutput: argv.includes('--github-output'),
    json: argv.includes('--json') || !argv.includes('--github-output'),
  };
}

function fail(message) {
  console.error(message);
  process.exit(1);
}

function isStableSemver(version) {
  return /^\d+\.\d+\.\d+$/.test(String(version));
}

function registryVersionExists(packageName, version) {
  const npmArgs = ['view', `${packageName}@${version}`, 'version', '--json'];
  const command = process.platform === 'win32' ? 'cmd' : 'npm';
  const commandArgs = process.platform === 'win32' ? ['/d', '/s', '/c', 'npm', ...npmArgs] : npmArgs;
  const result = spawnSync(command, commandArgs, {
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
    timeout: 60_000,
  });
  if (result.status === 0) return true;
  const combined = `${result.stdout ?? ''}\n${result.stderr ?? ''}`;
  if (/E404|404 Not Found|No match found/i.test(combined)) return false;
  const detail = result.error?.message ?? (combined.trim() || `exit ${result.status}`);
  throw new Error(`unable to check npm registry for ${packageName}@${version}: ${detail}`);
}

function setGithubOutput(values) {
  const outputPath = process.env.GITHUB_OUTPUT;
  if (!outputPath) return;
  const lines = Object.entries(values).map(([key, value]) => `${key}=${String(value).replace(/\r?\n/g, ' ')}`);
  fs.appendFileSync(outputPath, `${lines.join('\n')}\n`, 'utf8');
}

function plan() {
  const cliPackage = readCliPackageJson();
  const packageName = cliPackage.name;
  const sourceVersion = cliPackage.version;
  if (!packageName) fail('packages/npm/cli/package.json is missing name');
  if (!isStableSemver(sourceVersion)) {
    fail(`source package version must stay stable (x.y.z); got '${sourceVersion}'`);
  }
  const versionOverride = (process.env.MCPACE_VERSION_OVERRIDE ?? '').trim();
  if (versionOverride && !isStableSemver(versionOverride)) {
    fail(`MCPACE_VERSION_OVERRIDE must be stable x.y.z when set; got '${versionOverride}'`);
  }
  const baseVersion = versionOverride || sourceVersion;

  const ref = process.env.GITHUB_REF ?? '';
  const refName = process.env.GITHUB_REF_NAME ?? '';
  const eventName = process.env.GITHUB_EVENT_NAME ?? '';
  const runNumber = process.env.GITHUB_RUN_NUMBER ?? '0';
  const dryRun = String(process.env.MCPACE_PUBLISH_DRY_RUN ?? 'false').toLowerCase() === 'true';

  let channel = 'unsupported';
  let distTag = 'latest';
  let effectiveVersion = baseVersion;
  let reason = '';

  if (ref === 'refs/heads/dev') {
    channel = 'dev';
    distTag = 'dev';
    effectiveVersion = `${baseVersion}-dev.${runNumber}`;
    reason = 'dev branch publishes a unique prerelease version to the dev dist-tag';
  } else if (ref === 'refs/heads/main' || ref === 'refs/heads/master') {
    channel = 'stable';
    distTag = 'latest';
    reason = `${refName || 'main'} branch publishes the stable package version to latest when that version is absent from npm`;
  } else if (ref.startsWith('refs/tags/v')) {
    const tagVersion = refName.replace(/^v/, '');
    if (tagVersion !== baseVersion) {
      fail(`release tag ${refName} does not match package version ${baseVersion}`);
    }
    channel = 'stable';
    distTag = 'latest';
    reason = 'version tag publishes the stable package version to latest when that version is absent from npm';
  } else if (eventName === 'workflow_dispatch' && dryRun) {
    channel = 'dry-run';
    distTag = ref === 'refs/heads/dev' ? 'dev' : 'latest';
    reason = 'manual dry-run validates packaging without publishing';
  } else {
    reason = `ref '${ref || '<missing>'}' is not a publishable MCPace npm release ref`;
  }

  let alreadyPublished = false;
  if (channel !== 'unsupported' && !dryRun) {
    alreadyPublished = registryVersionExists(packageName, effectiveVersion);
  }
  const shouldPublish = channel !== 'unsupported' && (dryRun || !alreadyPublished);
  if (alreadyPublished) {
    reason = `${packageName}@${effectiveVersion} already exists on npm; skipping duplicate publish`;
  }

  return {
    schema: 'mcpace.npmPublishPlan.v1',
    packageName,
    sourceVersion,
    baseVersion,
    versionOverride: versionOverride || null,
    effectiveVersion,
    channel,
    distTag,
    dryRun,
    alreadyPublished,
    shouldPublish,
    ref,
    refName,
    eventName,
    runNumber,
    reason,
  };
}

try {
  const args = parseArgs(process.argv.slice(2));
  const report = plan();
  if (args.githubOutput) {
    setGithubOutput({
      package_name: report.packageName,
      source_version: report.sourceVersion,
      base_version: report.baseVersion,
      version_override: report.versionOverride ?? '',
      effective_version: report.effectiveVersion,
      channel: report.channel,
      dist_tag: report.distTag,
      dry_run: report.dryRun,
      already_published: report.alreadyPublished,
      should_publish: report.shouldPublish,
      reason: report.reason,
    });
  }
  if (args.json) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
  else process.stdout.write(`npm publish plan: ${report.shouldPublish ? 'publish' : 'skip'} ${report.packageName}@${report.effectiveVersion} (${report.distTag}) — ${report.reason}\n`);
  process.exitCode = 0;
} catch (error) {
  console.error(error?.stack ?? String(error));
  process.exitCode = 1;
}
