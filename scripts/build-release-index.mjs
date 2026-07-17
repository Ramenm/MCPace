#!/usr/bin/env node
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { readRegularFileStableSync, writeFileAtomicSync } from './lib/atomic-fs.mjs';
import { deriveProjectVersion, readJson, repoRoot } from './lib/project-metadata.mjs';

const args = process.argv.slice(2);
const jsonOutput = args.includes('--json');

function argValue(name, fallback = null) {
  const index = args.indexOf(name);
  return index >= 0 ? args[index + 1] ?? fallback : fallback;
}

function fail(message) {
  const report = {
    schema: 'mcpace.releaseIndexBuild.v1',
    status: 'failed',
    error: message,
  };
  if (jsonOutput) {
    process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
  } else {
    process.stderr.write(`${message}\n`);
  }
  process.exit(1);
}

function normalizeRelativePath(value) {
  const normalized = String(value).split(/[\\/]+/).join('/');
  if (!normalized || normalized.startsWith('/') || normalized.includes('\\') || normalized.split('/').some((part) => !part || part === '.' || part === '..')) {
    throw new Error(`unsafe release asset path: ${value}`);
  }
  return normalized;
}

function walkFiles(root) {
  const files = [];
  const stack = [root];
  while (stack.length > 0) {
    const current = stack.pop();
    for (const entry of fs.readdirSync(current, { withFileTypes: true }).sort((left, right) => left.name.localeCompare(right.name))) {
      const full = path.join(current, entry.name);
      const stat = fs.lstatSync(full);
      if (stat.isSymbolicLink()) {
        throw new Error(`refusing to index symbolic link asset: ${path.relative(root, full)}`);
      }
      if (stat.isDirectory()) {
        stack.push(full);
      } else if (stat.isFile()) {
        files.push(full);
      } else {
        throw new Error(`refusing to index non-regular asset: ${path.relative(root, full)}`);
      }
    }
  }
  return files.sort((left, right) => normalizeRelativePath(path.relative(root, left)).localeCompare(normalizeRelativePath(path.relative(root, right))));
}

function shouldSkip(relativePath, version) {
  const basename = path.posix.basename(relativePath);
  return basename === `mcpace-v${version}-checksums.sha256`
    || basename === `mcpace-v${version}-release-assets.json`
    || basename === 'checksums.sha256'
    || basename === 'release-assets.json';
}

function sha256File(filePath) {
  const { data, stat } = readRegularFileStableSync(filePath, { maxBytes: Number(process.env.MCPACE_RELEASE_ASSET_MAX_BYTES || 512 * 1024 * 1024) });
  return {
    sha256: crypto.createHash('sha256').update(data).digest('hex'),
    bytes: Number(stat.size),
  };
}

function targetPackageName(target) {
  return target.packageName ?? target.npmPackage ?? `@mcpace/cli-${target.key}`;
}

function targetForAssetName(fileName, targets) {
  return targets.find((target) => fileName.includes(`-${target.key}.`) || fileName.includes(`-${target.key}-`)) ?? null;
}

function installerExtensionForTarget(target) {
  if (target.platform === 'win32') return 'msi';
  if (target.platform === 'darwin') return 'pkg';
  if (target.platform === 'linux' && target.libc?.includes('glibc')) return 'deb';
  throw new Error(`no installer extension is configured for target '${target.key}'`);
}

function installerKindForTarget(target) {
  if (target.platform === 'win32') return 'windows-msi';
  if (target.platform === 'darwin') return 'macos-pkg';
  if (target.platform === 'linux') return 'debian-package';
  return 'unknown';
}

function installCommandForTarget(target, assetName) {
  if (target.platform === 'win32') return `msiexec /i ${assetName}`;
  if (target.platform === 'darwin') return `sudo installer -pkg ${assetName} -target /`;
  if (target.platform === 'linux') return `sudo apt install ./${assetName}`;
  return null;
}

function buildIndex({ assetDir, outDir }) {
  const version = deriveProjectVersion();
  const releaseTargets = readJson('release-targets.json');
  const enabledTargets = (releaseTargets.targets ?? []).filter((target) => target.publishEnabled !== false);
  const files = walkFiles(assetDir)
    .map((fullPath) => ({
      fullPath,
      relativePath: normalizeRelativePath(path.relative(assetDir, fullPath)),
      fileName: path.basename(fullPath),
    }))
    .filter((entry) => !shouldSkip(entry.relativePath, version));

  const duplicateBasenames = [];
  const seenBasenames = new Set();
  for (const entry of files) {
    if (seenBasenames.has(entry.fileName)) duplicateBasenames.push(entry.fileName);
    seenBasenames.add(entry.fileName);
  }
  if (duplicateBasenames.length > 0) {
    throw new Error(`duplicate release asset names would collide on GitHub: ${[...new Set(duplicateBasenames)].sort().join(', ')}`);
  }

  const assets = files.map((entry) => {
    const digest = sha256File(entry.fullPath);
    const target = targetForAssetName(entry.fileName, enabledTargets);
    return {
      name: entry.fileName,
      path: entry.relativePath,
      bytes: digest.bytes,
      sha256: digest.sha256,
      target: target ? {
        key: target.key,
        platform: target.platform,
        arch: target.arch,
        rustTarget: target.rustTarget,
        libc: target.libc ?? null,
        glibcBaselineImage: target.glibcBaselineImage ?? null,
        glibcBaseline: target.glibcBaseline ?? null,
        binaryName: target.binaryName ?? (target.platform === 'win32' ? 'mcpace.exe' : 'mcpace'),
        npmPackage: targetPackageName(target),
        installerKind: installerKindForTarget(target),
        installCommand: installCommandForTarget(target, entry.fileName),
      } : null,
    };
  });

  const missingNativeInstallerTargets = enabledTargets
    .filter((target) => !assets.some((asset) => asset.target?.key === target.key && asset.name.endsWith(`.${installerExtensionForTarget(target)}`)))
    .map((target) => target.key);
  if (missingNativeInstallerTargets.length > 0) {
    throw new Error(`missing native GitHub release installers for targets: ${missingNativeInstallerTargets.join(', ')}`);
  }

  const index = {
    schema: 'mcpace.releaseAssets.v1',
    generatedAt: new Date().toISOString(),
    version,
    packageName: '@mcpace/cli',
    updatePolicy: {
      mode: 'package-manager-managed',
      defaultCheck: 'mcpace advanced update check --source npm',
      recommendedInstall: 'npm install -g @mcpace/cli@latest',
      directGitHubInstallers: 'manual-upgrade-only',
      selfRewrite: false,
      futureOsUpdateChannels: {
        linux: 'signed apt repository for .deb upgrades',
        windows: 'signed MSI plus winget package metadata',
        macos: 'signed and notarized PKG plus Homebrew cask metadata',
      },
      rationale: 'GitHub release assets are installable MSI/DEB/PKG files; automatic updates should use npm optional native packages until signed OS package repositories or signed self-update support exists.',
    },
    ubuntu: {
      x64: 'linux-x64-gnu',
      arm64: 'linux-arm64-gnu',
      packageFormat: 'deb',
      glibcBaselineImage: 'ubuntu:22.04',
      note: 'Ubuntu uses glibc; install the linux-*-gnu .deb packages. Linux release binaries are built inside the Ubuntu 22.04 glibc baseline container for compatibility with Ubuntu 22.04+ and newer glibc distributions. Alpine/musl targets stay planned until dedicated proof exists.',
    },
    assets,
  };

  fs.mkdirSync(outDir, { recursive: true });
  const checksumsName = `mcpace-v${version}-checksums.sha256`;
  const manifestName = `mcpace-v${version}-release-assets.json`;
  const checksumsPath = path.join(outDir, checksumsName);
  const manifestPath = path.join(outDir, manifestName);
  const checksumText = assets
    .map((asset) => `${asset.sha256}  ${asset.name}`)
    .sort()
    .join('\n');
  writeFileAtomicSync(checksumsPath, `${checksumText}\n`, { mode: 0o644 });
  writeFileAtomicSync(manifestPath, `${JSON.stringify(index, null, 2)}\n`, { mode: 0o644 });

  return {
    schema: 'mcpace.releaseIndexBuild.v1',
    status: 'pass',
    assetDir: path.relative(repoRoot, assetDir).split(path.sep).join('/') || '.',
    outDir: path.relative(repoRoot, outDir).split(path.sep).join('/') || '.',
    assetCount: assets.length,
    checksumsPath: path.relative(repoRoot, checksumsPath).split(path.sep).join('/'),
    manifestPath: path.relative(repoRoot, manifestPath).split(path.sep).join('/'),
    missingNativeInstallerTargets,
    assets,
  };
}

try {
  const assetDir = path.resolve(argValue('--asset-dir', path.join(repoRoot, 'dist', 'release-assets')));
  const outDir = path.resolve(argValue('--out-dir', assetDir));
  if (!fs.existsSync(assetDir) || !fs.statSync(assetDir).isDirectory()) {
    throw new Error(`asset directory does not exist: ${assetDir}`);
  }
  const report = buildIndex({ assetDir, outDir });
  if (jsonOutput) {
    process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
  } else {
    process.stdout.write(`Built ${report.checksumsPath}\nBuilt ${report.manifestPath}\n`);
  }
} catch (error) {
  fail(error?.message ?? String(error));
}
