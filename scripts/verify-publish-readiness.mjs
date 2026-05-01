#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { deriveProjectVersion, repoRoot } from './lib/project-metadata.mjs';
import { PLATFORM_PACKAGE_TARGETS } from './lib/npm-platform-packages.mjs';

const DEFAULT_WORKFLOW = path.join('.github', 'workflows', 'publish-npm.yml');
const DEFAULT_ENVIRONMENT = 'npm-publish';

function normalizeRepositoryUrl(value) {
  const trimmed = String(value || '').trim();
  if (!trimmed) return null;
  return trimmed.replace(/\.git$/i, '').replace(/^git\+/, '');
}

function repositoryUrlFromGithubEnv() {
  const server = process.env.GITHUB_SERVER_URL || 'https://github.com';
  const repository = process.env.GITHUB_REPOSITORY;
  return repository ? normalizeRepositoryUrl(`${server}/${repository}`) : null;
}

function parseArgs(argv) {
  const parsed = { json: false, repositoryUrl: repositoryUrlFromGithubEnv(), workflow: DEFAULT_WORKFLOW, environment: DEFAULT_ENVIRONMENT };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--json': parsed.json = true; break;
      case '--repository-url': parsed.repositoryUrl = normalizeRepositoryUrl(argv[++index] || ''); break;
      case '--workflow': parsed.workflow = argv[++index] || DEFAULT_WORKFLOW; break;
      case '--environment': parsed.environment = argv[++index] || DEFAULT_ENVIRONMENT; break;
      default: throw new Error(`unsupported verify-publish-readiness argument: ${token}`);
    }
  }
  return parsed;
}

function readJson(relativePath) {
  return JSON.parse(fs.readFileSync(path.join(repoRoot, relativePath), 'utf8'));
}

function readText(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

function rawRepositoryUrl(repository) {
  if (typeof repository === 'string') return repository.trim();
  if (repository && typeof repository.url === 'string') return repository.url.trim();
  return null;
}

function packageEntries() {
  return [
    { role: 'main', name: '@mcpace/cli', path: path.join('packages', 'npm', 'cli', 'package.json'), directory: 'packages/npm/cli' },
    ...PLATFORM_PACKAGE_TARGETS.map((target) => ({ role: 'platform', name: target.packageName, path: path.join('packages', 'npm', `cli-${target.key}`, 'package.json'), directory: `packages/npm/cli-${target.key}` }))
  ];
}

function validatePackage(entry, expectedVersion, expectedRepositoryUrl) {
  const issues = [];
  const warnings = [];
  const manifest = readJson(entry.path);
  const repository = manifest.repository;
  const repositoryUrl = rawRepositoryUrl(repository);
  const repositoryDirectory = repository && typeof repository === 'object' ? repository.directory : null;

  if (manifest.name !== entry.name) issues.push(`${entry.path} name must be ${entry.name}`);
  if (manifest.version !== expectedVersion) issues.push(`${entry.path} version must be ${expectedVersion}`);
  if (manifest.license !== 'Apache-2.0') issues.push(`${entry.path} license must be Apache-2.0`);
  if (manifest.publishConfig?.access !== 'public') issues.push(`${entry.path} publishConfig.access must be public`);
  if (!expectedRepositoryUrl) {
    warnings.push('repository URL is not configured; strict trusted-publishing metadata checks require --repository-url or GitHub Actions GITHUB_REPOSITORY');
  } else {
    if (repositoryUrl !== expectedRepositoryUrl) issues.push(`${entry.path} repository.url must exactly match ${expectedRepositoryUrl}`);
    if (repositoryDirectory !== entry.directory) issues.push(`${entry.path} repository.directory must be ${entry.directory}`);
  }

  const status = issues.length > 0 ? 'fail' : warnings.length > 0 ? 'pending' : 'pass';
  return { role: entry.role, name: entry.name, path: entry.path, status, issues, warnings };
}

function validateWorkflow(relativePath, environment) {
  const issues = [];
  const workflow = readText(relativePath);
  const checks = [
    [/name:\s*publish-npm/, 'publish workflow must be named publish-npm for npm Trusted Publisher configuration'],
    [/publish:/, 'missing publish job'],
    [/id-token:\s*write/, 'publish workflow must grant id-token: write'],
    [new RegExp(`environment:\\s*${environment.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}`), `publish workflow must use ${environment} environment`],
    [/registry-url:\s*https:\/\/registry\.npmjs\.org/, 'publish workflow must configure npm registry-url'],
    [/package-manager-cache:\s*false/, 'publish workflow must disable automatic npm caching in the trusted-publishing job'],
    [/npm exec --yes --package=npm@11\.12\.1 -- npm --version/, 'publish workflow must verify the exact npm version without global installation'],
    [/MCPACE_NPM_EXEC_PACKAGE:\s*npm@11\.12\.1/, 'publish workflow must run publish commands through the exact npm package'],
    [/gh release download/, 'publish workflow must download prebuilt npm artifacts from a GitHub Release'],
    [/node scripts\/verify-release-checksums\.mjs --json --artifact-dir dist\/npm/, 'publish workflow must verify downloaded npm artifacts against release checksums'],
    [/node scripts\/publish-npm-artifacts\.mjs --json --artifact-dir dist\/npm/, 'publish workflow must publish prebuilt npm artifacts'],
    [/node scripts\/verify-publish-readiness\.mjs --json/, 'publish workflow must run publish readiness gate before npm publish']
  ];
  for (const [pattern, message] of checks) if (!pattern.test(workflow)) issues.push(message);
  return { path: relativePath, status: issues.length === 0 ? 'pass' : 'fail', issues };
}

export function verifyPublishReadiness(options = {}) {
  const expectedVersion = deriveProjectVersion();
  const repositoryUrl = normalizeRepositoryUrl(options.repositoryUrl || repositoryUrlFromGithubEnv() || '');
  const workflow = validateWorkflow(options.workflow || DEFAULT_WORKFLOW, options.environment || DEFAULT_ENVIRONMENT);
  const packages = packageEntries().map((entry) => validatePackage(entry, expectedVersion, repositoryUrl));
  const issues = [workflow, ...packages].flatMap((entry) => entry.issues);
  const warnings = packages.flatMap((entry) => entry.warnings || []);
  const status = issues.length > 0 ? 'fail' : warnings.length > 0 ? 'pending' : 'pass';
  return { version: expectedVersion, status, repositoryUrl, workflow, packages, issues, warnings };
}

function isCliInvocation() {
  const entry = process.argv[1];
  return entry ? pathToFileURL(path.resolve(entry)).href === import.meta.url : false;
}

function main() {
  try {
    const parsed = parseArgs(process.argv.slice(2));
    const report = verifyPublishReadiness(parsed);
    if (parsed.json) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
    else if (report.status === 'pass') process.stdout.write(`publish readiness verified for ${report.repositoryUrl}\n`);
    else if (report.status === 'pending') process.stdout.write(`publish readiness pending: ${report.warnings[0]}\n`);
    else process.stderr.write(`${report.issues.join('\n')}\n`);
    if (report.status === 'fail') process.exit(1);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exit(1);
  }
}

if (isCliInvocation()) main();
