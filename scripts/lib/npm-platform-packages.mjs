#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { repoRoot } from './project-metadata.mjs';
import {
  enabledReleaseTargets,
  releaseTargetByKey,
  releaseTargetByPackageName,
  releaseTargetsManifest
} from './release-targets.mjs';

const manifest = releaseTargetsManifest();

export const PLATFORM_PACKAGE_SCOPE = manifest.packageScope || '@mcpace';
export const PLATFORM_PACKAGE_PREFIX = `${PLATFORM_PACKAGE_SCOPE}/cli-`;
export const PLATFORM_PACKAGE_ROOT = path.join(repoRoot, 'packages', 'npm');
export const PLATFORM_PACKAGE_TARGETS = enabledReleaseTargets(manifest);

export function platformPackageDir(target) {
  return path.join(PLATFORM_PACKAGE_ROOT, `cli-${target.key}`);
}

export function platformPackageBinPath(target) {
  return path.join(platformPackageDir(target), 'bin', target.binaryName);
}

export function targetByKey(targetKey) {
  return releaseTargetByKey(targetKey || '');
}

export function targetByPackageName(packageName) {
  return releaseTargetByPackageName(packageName);
}

export function defaultCargoBinaryPath(target) {
  return path.join(repoRoot, 'target', target.triple, 'release', target.binaryName);
}

export function expectedOptionalDependencies(version) {
  return Object.fromEntries(
    PLATFORM_PACKAGE_TARGETS.map((target) => [target.packageName, version]).sort(([left], [right]) => left.localeCompare(right))
  );
}

export function platformPackageJson(target, version, options = {}) {
  const repositoryUrl = options.repositoryUrl || null;
  return {
    name: target.packageName,
    version,
    description: `Platform-specific MCPace native binary for ${target.key}.`,
    license: 'Apache-2.0',
    os: target.os,
    cpu: target.cpu,
    ...(target.libc ? { libc: target.libc } : {}),
    files: ['bin', 'README.md', 'LICENSE'],
    engines: {
      node: '>=22.0.0'
    },
    publishConfig: {
      access: 'public'
    },
    ...(repositoryUrl
      ? {
          repository: {
            type: 'git',
            url: repositoryUrl,
            directory: `packages/npm/cli-${target.key}`
          }
        }
      : {}),
    keywords: ['mcpace', 'mcp', 'cli', 'rust', 'native-binary', target.key]
  };
}

export function platformPackageReadme(target) {
  return `# ${target.packageName}\n\nPlatform-specific native binary package for \`@mcpace/cli\`.\n\nTarget key: \`${target.key}\`\nRust target: \`${target.triple}\`\nGitHub runner: \`${target.runner}\`\n\nDo not install this package directly. Install \`@mcpace/cli\`; package managers use optional dependencies to select the matching native package for the current platform.\n`;
}

export function ensurePlatformPackageScaffold(target, version, options = {}) {
  const dir = platformPackageDir(target);
  fs.mkdirSync(path.join(dir, 'bin'), { recursive: true });
  fs.writeFileSync(
    path.join(dir, 'package.json'),
    `${JSON.stringify(platformPackageJson(target, version, options), null, 2)}\n`,
    'utf8'
  );
  fs.writeFileSync(path.join(dir, 'README.md'), platformPackageReadme(target), 'utf8');
  const licenseText = fs.readFileSync(path.join(repoRoot, 'LICENSE'), 'utf8').replace(/\r\n/g, '\n').replace(/\r/g, '\n');
  fs.writeFileSync(path.join(dir, 'LICENSE'), licenseText, 'utf8');
  const keepPath = path.join(dir, 'bin', '.gitkeep');
  if (!fs.existsSync(keepPath) && !fs.existsSync(platformPackageBinPath(target))) {
    fs.writeFileSync(keepPath, '', 'utf8');
  }
  return dir;
}
