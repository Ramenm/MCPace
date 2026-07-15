#!/usr/bin/env node
import crypto from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { spawnSync } from 'node:child_process';
import { gzipSync } from 'node:zlib';
import { copyRegularFileNoFollowSync, readRegularFileStableSync, writeFileAtomicSync } from './lib/atomic-fs.mjs';
import { deriveProjectVersion, readJson, repoRoot } from './lib/project-metadata.mjs';

const args = process.argv.slice(2);
const jsonOutput = args.includes('--json');
const wixSourceOnly = args.includes('--wix-source-only') || args.includes('--no-external-build');
const FIXED_MTIME_SECONDS = 0;
const TAR_BLOCK_SIZE = 512;
const WINDOWS_AGENT_LAUNCHER_NAME = 'mcpace-agent-launcher.exe';

function argValue(name, fallback = null) {
  const index = args.indexOf(name);
  return index >= 0 ? args[index + 1] ?? fallback : fallback;
}

function fail(message) {
  const report = {
    schema: 'mcpace.nativeInstallerAssetBuild.v1',
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

function validateTarget(releaseTargets, key) {
  const target = (releaseTargets.targets ?? []).find((candidate) => candidate.key === key);
  if (!target) throw new Error(`unknown release target '${key}'`);
  if (target.publishEnabled === false) throw new Error(`release target '${key}' is not publish-enabled`);
  if (!target.platform || !target.arch || !target.rustTarget) {
    throw new Error(`release target '${key}' is missing platform, arch, or rustTarget metadata`);
  }
  return target;
}

function binaryNameForTarget(target) {
  return target.binaryName ?? (target.platform === 'win32' ? 'mcpace.exe' : 'mcpace');
}

function validateBinaryForTarget(binaryPath, target) {
  let stat;
  try {
    stat = fs.lstatSync(binaryPath);
  } catch (error) {
    throw new Error(`native binary '${binaryPath}' is not readable: ${error?.message ?? error}`);
  }
  if (stat.isSymbolicLink()) throw new Error(`native binary '${binaryPath}' must not be a symbolic link`);
  if (!stat.isFile()) throw new Error(`native binary '${binaryPath}' must be a regular file`);
  if (target.platform !== 'win32' && process.platform !== 'win32' && (Number(stat.mode) & 0o111) === 0) {
    throw new Error(`native binary '${binaryPath}' must have an executable bit for target '${target.key}'`);
  }
  if (target.platform === 'win32' && !String(binaryPath).toLowerCase().endsWith('.exe')) {
    throw new Error(`native binary for Windows target '${target.key}' must use .exe extension`);
  }
}

function validateSidecarBinaryForTarget(binaryPath, target, name) {
  let stat;
  try {
    stat = fs.lstatSync(binaryPath);
  } catch (error) {
    throw new Error(`required native sidecar '${name}' for target '${target.key}' is not readable at '${binaryPath}': ${error?.message ?? error}`);
  }
  if (stat.isSymbolicLink()) throw new Error(`required native sidecar '${binaryPath}' must not be a symbolic link`);
  if (!stat.isFile()) throw new Error(`required native sidecar '${binaryPath}' must be a regular file`);
  if (target.platform !== 'win32' && process.platform !== 'win32' && (Number(stat.mode) & 0o111) === 0) {
    throw new Error(`required native sidecar '${binaryPath}' must have an executable bit for target '${target.key}'`);
  }
  if (target.platform === 'win32' && !String(binaryPath).toLowerCase().endsWith('.exe')) {
    throw new Error(`required native sidecar for Windows target '${target.key}' must use .exe extension`);
  }
  return stat;
}

function sidecarBinariesForTarget(binaryPath, target) {
  if (target.platform !== 'win32') return [];
  const launcherPath = path.join(path.dirname(binaryPath), WINDOWS_AGENT_LAUNCHER_NAME);
  const stat = validateSidecarBinaryForTarget(launcherPath, target, WINDOWS_AGENT_LAUNCHER_NAME);
  return [{
    name: WINDOWS_AGENT_LAUNCHER_NAME,
    sourcePath: launcherPath,
    size: stat.size,
    purpose: 'hidden Windows autostart launcher that starts MCPace Agent without a terminal window',
  }];
}

function installerExtension(target) {
  if (target.platform === 'win32') return 'msi';
  if (target.platform === 'darwin') return 'pkg';
  if (target.platform === 'linux' && target.libc?.includes('glibc')) return 'deb';
  throw new Error(`no installer format is configured for release target '${target.key}'`);
}

function installerKind(target) {
  if (target.platform === 'win32') return 'windows-msi';
  if (target.platform === 'darwin') return 'macos-pkg';
  if (target.platform === 'linux') return 'debian-package';
  return 'unknown';
}

function debArchitecture(target) {
  if (target.arch === 'x64') return 'amd64';
  if (target.arch === 'arm64') return 'arm64';
  throw new Error(`unsupported Debian architecture for target '${target.key}': ${target.arch}`);
}

function wixArchitecture(target) {
  if (target.arch === 'x64') return 'x64';
  if (target.arch === 'arm64') return 'arm64';
  throw new Error(`unsupported WiX architecture for target '${target.key}': ${target.arch}`);
}

function sha256File(filePath) {
  const { data } = readRegularFileStableSync(filePath, { maxBytes: Number(process.env.MCPACE_RELEASE_ASSET_MAX_BYTES || 512 * 1024 * 1024) });
  return crypto.createHash('sha256').update(data).digest('hex');
}

function normalizePackagePath(value) {
  const normalized = String(value).split(/[\\/]+/).join('/').replace(/^\/+/, '');
  if (!normalized || normalized.startsWith('/') || normalized.includes('\\') || normalized.split('/').some((part) => !part || part === '.' || part === '..')) {
    throw new Error(`unsafe package path: ${value}`);
  }
  return normalized;
}

function writeOctal(header, offset, length, value) {
  const encoded = value.toString(8).padStart(length - 1, '0');
  if (encoded.length > length - 1) throw new Error(`tar numeric field too large: ${value}`);
  header.write(encoded, offset, length - 1, 'ascii');
  header[offset + length - 1] = 0;
}

function writeString(header, offset, length, value) {
  const data = Buffer.from(value, 'utf8');
  if (data.length > length) throw new Error(`tar string field too long: ${value}`);
  data.copy(header, offset);
}

function splitTarName(name) {
  const encoded = Buffer.from(name, 'utf8');
  if (encoded.length <= 100) return { name, prefix: '' };
  const parts = name.split('/');
  for (let index = 1; index < parts.length; index += 1) {
    const prefix = parts.slice(0, index).join('/');
    const remainder = parts.slice(index).join('/');
    if (Buffer.from(prefix, 'utf8').length <= 155 && Buffer.from(remainder, 'utf8').length <= 100) {
      return { name: remainder, prefix };
    }
  }
  throw new Error(`tar path is too long for ustar: ${name}`);
}

function tarHeader(entryName, options) {
  const header = Buffer.alloc(TAR_BLOCK_SIZE, 0);
  const baseName = options.type === 'directory' ? String(entryName).replace(/\/+$/u, '') : entryName;
  const normalizedName = `${normalizePackagePath(baseName)}${options.type === 'directory' ? '/' : ''}`;
  const { name, prefix } = splitTarName(normalizedName);
  const mode = options.mode;
  const size = options.type === 'directory' ? 0 : options.size;
  writeString(header, 0, 100, name);
  writeOctal(header, 100, 8, mode);
  writeOctal(header, 108, 8, 0);
  writeOctal(header, 116, 8, 0);
  writeOctal(header, 124, 12, size);
  writeOctal(header, 136, 12, FIXED_MTIME_SECONDS);
  header.fill(0x20, 148, 156);
  header[156] = options.type === 'directory' ? '5'.charCodeAt(0) : '0'.charCodeAt(0);
  writeString(header, 257, 6, 'ustar');
  writeString(header, 263, 2, '00');
  writeString(header, 265, 32, 'root');
  writeString(header, 297, 32, 'root');
  if (prefix) writeString(header, 345, 155, prefix);
  let checksum = 0;
  for (const byte of header) checksum += byte;
  const checksumText = checksum.toString(8).padStart(6, '0');
  header.write(checksumText, 148, 6, 'ascii');
  header[154] = 0;
  header[155] = 0x20;
  return header;
}

function padBlock(data) {
  const remainder = data.length % TAR_BLOCK_SIZE;
  return remainder === 0 ? Buffer.alloc(0) : Buffer.alloc(TAR_BLOCK_SIZE - remainder, 0);
}

function createTarGz(entries) {
  const parts = [];
  for (const entry of entries) {
    if (entry.type === 'directory') {
      const name = entry.path.endsWith('/') ? entry.path : `${entry.path}/`;
      parts.push(tarHeader(name, { type: 'directory', mode: entry.mode ?? 0o755, size: 0 }));
      continue;
    }
    const data = Buffer.isBuffer(entry.data) ? entry.data : Buffer.from(String(entry.data), 'utf8');
    parts.push(tarHeader(entry.path, { type: 'file', mode: entry.mode ?? 0o644, size: data.length }), data, padBlock(data));
  }
  parts.push(Buffer.alloc(TAR_BLOCK_SIZE * 2, 0));
  return gzipSync(Buffer.concat(parts), { level: 9, mtime: 0 });
}

function arMember(name, data, mode = 0o100644) {
  const payload = Buffer.isBuffer(data) ? data : Buffer.from(String(data), 'utf8');
  if (Buffer.byteLength(name, 'ascii') > 15) throw new Error(`ar member name too long: ${name}`);
  const header = Buffer.alloc(60, 0x20);
  header.write(`${name}/`, 0, 'ascii');
  header.write(String(FIXED_MTIME_SECONDS), 16, 'ascii');
  header.write('0', 28, 'ascii');
  header.write('0', 34, 'ascii');
  header.write(mode.toString(8), 40, 'ascii');
  header.write(String(payload.length), 48, 'ascii');
  header.write('`\n', 58, 'ascii');
  return Buffer.concat([header, payload, payload.length % 2 === 1 ? Buffer.from('\n') : Buffer.alloc(0)]);
}

function createDebianPackage(packagePath, target, binaryPath, version) {
  const binaryName = binaryNameForTarget(target);
  const { data: binaryData } = readRegularFileStableSync(binaryPath, { maxBytes: Number(process.env.MCPACE_NATIVE_BINARY_MAX_BYTES || 128 * 1024 * 1024) });
  const readme = fs.existsSync(path.join(repoRoot, 'README.md')) ? fs.readFileSync(path.join(repoRoot, 'README.md')) : Buffer.from('# MCPace\n');
  const license = fs.readFileSync(path.join(repoRoot, 'LICENSE'));
  const notice = fs.readFileSync(path.join(repoRoot, 'NOTICE'));
  const debVersion = `${String(version).replace(/-/g, '~')}-1`;
  const installedSizeKb = Math.max(1, Math.ceil((binaryData.length + readme.length + license.length + notice.length) / 1024));
  const control = [
    'Package: mcpace',
    `Version: ${debVersion}`,
    'Section: utils',
    'Priority: optional',
    `Architecture: ${debArchitecture(target)}`,
    'Maintainer: MCPace Maintainers <noreply@github.com>',
    'Homepage: https://github.com/Ramenm/MCPace',
    `Installed-Size: ${installedSizeKb}`,
    'Description: Dynamic MCP adapter command-line interface',
    ' MCPace is a CLI and runtime for routing clients to configured MCP servers.',
    '',
  ].join('\n');
  const controlTar = createTarGz([
    { path: 'control', data: control, mode: 0o644 },
  ]);
  const docRoot = 'usr/share/doc/mcpace';
  const dataTar = createTarGz([
    { type: 'directory', path: 'usr', mode: 0o755 },
    { type: 'directory', path: 'usr/bin', mode: 0o755 },
    { type: 'directory', path: 'usr/share', mode: 0o755 },
    { type: 'directory', path: 'usr/share/doc', mode: 0o755 },
    { type: 'directory', path: docRoot, mode: 0o755 },
    { path: `usr/bin/${binaryName}`, data: binaryData, mode: 0o755 },
    { path: `${docRoot}/README.md`, data: readme, mode: 0o644 },
    { path: `${docRoot}/LICENSE`, data: license, mode: 0o644 },
    { path: `${docRoot}/NOTICE`, data: notice, mode: 0o644 },
  ]);
  const deb = Buffer.concat([
    Buffer.from('!<arch>\n', 'ascii'),
    arMember('debian-binary', '2.0\n'),
    arMember('control.tar.gz', controlTar),
    arMember('data.tar.gz', dataTar),
  ]);
  writeFileAtomicSync(packagePath, deb, { mode: 0o644 });
}

function xmlEscape(value) {
  return String(value)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&apos;');
}

function uuidFromName(name) {
  const namespace = Buffer.from('1a85f7b348d0478ea54b7dcf0cc4aa0b', 'hex');
  const hash = crypto.createHash('sha1').update(namespace).update(String(name)).digest();
  hash[6] = (hash[6] & 0x0f) | 0x50;
  hash[8] = (hash[8] & 0x3f) | 0x80;
  const hex = hash.subarray(0, 16).toString('hex');
  return `{${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}}`;
}

function writeWindowsWixSource(wxsPath, target, binaryPath, version, sidecars) {
  const upgradeCode = uuidFromName('mcpace-upgrade-code');
  const exeComponentGuid = uuidFromName(`mcpace-${target.arch}-exe-component`);
  const agentLauncherComponentGuid = uuidFromName(`mcpace-${target.arch}-agent-launcher-component`);
  const windowsLauncher = sidecars.find((sidecar) => sidecar.name === WINDOWS_AGENT_LAUNCHER_NAME);
  if (!windowsLauncher) throw new Error(`missing required ${WINDOWS_AGENT_LAUNCHER_NAME} sidecar for Windows MSI source`);
  const licenseComponentGuid = uuidFromName(`mcpace-${target.arch}-license-component`);
  const noticeComponentGuid = uuidFromName(`mcpace-${target.arch}-notice-component`);
  const readmeComponentGuid = uuidFromName(`mcpace-${target.arch}-readme-component`);
  const readmePath = path.join(repoRoot, 'README.md');
  const licensePath = path.join(repoRoot, 'LICENSE');
  const noticePath = path.join(repoRoot, 'NOTICE');
  const wxs = `<?xml version="1.0" encoding="UTF-8"?>
<Wix xmlns="http://wixtoolset.org/schemas/v4/wxs">
  <Package Name="MCPace" Manufacturer="MCPace" Version="${xmlEscape(version)}" UpgradeCode="${upgradeCode}" Scope="perMachine">
    <MajorUpgrade DowngradeErrorMessage="A newer version of MCPace is already installed." />
    <MediaTemplate EmbedCab="yes" />
    <Feature Id="MainFeature" Title="MCPace" Level="1">
      <ComponentGroupRef Id="MCPaceComponents" />
    </Feature>
  </Package>
  <Fragment>
    <StandardDirectory Id="ProgramFilesFolder">
      <Directory Id="INSTALLFOLDER" Name="MCPace" />
    </StandardDirectory>
  </Fragment>
  <Fragment>
    <ComponentGroup Id="MCPaceComponents" Directory="INSTALLFOLDER">
      <Component Id="MCPaceExe" Guid="${exeComponentGuid}">
        <File Id="MCPaceExeFile" Source="${xmlEscape(binaryPath)}" Name="mcpace.exe" KeyPath="yes" />
        <Environment Id="MCPacePath" Name="PATH" Value="[INSTALLFOLDER]" Permanent="no" Part="last" Action="set" System="yes" />
      </Component>
      <Component Id="MCPaceAgentLauncher" Guid="${agentLauncherComponentGuid}">
        <File Id="MCPaceAgentLauncherFile" Source="${xmlEscape(windowsLauncher.sourcePath)}" Name="mcpace-agent-launcher.exe" KeyPath="yes" />
      </Component>
      <Component Id="MCPaceLicense" Guid="${licenseComponentGuid}">
        <File Id="MCPaceLicenseFile" Source="${xmlEscape(licensePath)}" Name="LICENSE" KeyPath="yes" />
      </Component>
      <Component Id="MCPaceNotice" Guid="${noticeComponentGuid}">
        <File Id="MCPaceNoticeFile" Source="${xmlEscape(noticePath)}" Name="NOTICE" KeyPath="yes" />
      </Component>
      <Component Id="MCPaceReadme" Guid="${readmeComponentGuid}">
        <File Id="MCPaceReadmeFile" Source="${xmlEscape(readmePath)}" Name="README.md" KeyPath="yes" />
      </Component>
    </ComponentGroup>
  </Fragment>
</Wix>
`;
  writeFileAtomicSync(wxsPath, wxs, { mode: 0o644 });
}

function runCommand(command, commandArgs, options = {}) {
  const result = spawnSync(command, commandArgs, {
    cwd: options.cwd ?? repoRoot,
    encoding: 'utf8',
    windowsHide: true,
    env: { ...process.env, ...(options.env ?? {}) },
  });
  if (result.status !== 0) {
    const detail = [result.stdout, result.stderr].filter(Boolean).join('\n').trim();
    throw new Error(`${command} ${commandArgs.join(' ')} failed${detail ? `:\n${detail}` : ''}`);
  }
  return result;
}

function createWindowsMsi(packagePath, target, binaryPath, version, workDir) {
  const sidecars = sidecarBinariesForTarget(binaryPath, target);
  const wxsPath = wixSourceOnly
    ? packagePath.replace(/\.msi$/u, '.wxs')
    : path.join(workDir, `mcpace-${target.key}.wxs`);
  writeWindowsWixSource(wxsPath, target, binaryPath, version, sidecars);
  if (wixSourceOnly) {
    return { built: false, wixSourcePath: wxsPath, sidecarBinaries: sidecars.map(sidecarReport) };
  }
  runCommand('wix', ['build', wxsPath, '-arch', wixArchitecture(target), '-o', packagePath]);
  return { built: true, wixSourcePath: wxsPath, sidecarBinaries: sidecars.map(sidecarReport) };
}

function sidecarReport(sidecar) {
  return {
    name: sidecar.name,
    purpose: sidecar.purpose,
    sourcePath: path.relative(repoRoot, sidecar.sourcePath).split(path.sep).join('/'),
    bytes: sidecar.size,
  };
}

function createMacPkg(packagePath, target, binaryPath, version, workDir) {
  if (process.platform !== 'darwin') {
    throw new Error(`macOS .pkg installers must be built on a macOS runner for target '${target.key}'`);
  }
  const root = path.join(workDir, 'pkgroot');
  const binDir = path.join(root, 'usr', 'local', 'bin');
  const docDir = path.join(root, 'usr', 'local', 'share', 'doc', 'mcpace');
  fs.mkdirSync(binDir, { recursive: true });
  fs.mkdirSync(docDir, { recursive: true });
  copyRegularFileNoFollowSync(binaryPath, path.join(binDir, 'mcpace'), { maxBytes: Number(process.env.MCPACE_NATIVE_BINARY_MAX_BYTES || 128 * 1024 * 1024) });
  fs.chmodSync(path.join(binDir, 'mcpace'), 0o755);
  copyRegularFileNoFollowSync(path.join(repoRoot, 'LICENSE'), path.join(docDir, 'LICENSE'), { maxBytes: 1024 * 1024 });
  copyRegularFileNoFollowSync(path.join(repoRoot, 'NOTICE'), path.join(docDir, 'NOTICE'), { maxBytes: 1024 * 1024 });
  copyRegularFileNoFollowSync(path.join(repoRoot, 'README.md'), path.join(docDir, 'README.md'), { maxBytes: 5 * 1024 * 1024 });
  runCommand('pkgbuild', [
    '--root', root,
    '--identifier', 'io.github.ramenm.mcpace',
    '--version', version,
    '--install-location', '/',
    '--ownership', 'recommended',
    packagePath,
  ]);
  return { built: true };
}

function buildInstaller(target, binaryPath, outDir, version) {
  const extension = installerExtension(target);
  const installerName = `mcpace-v${version}-${target.key}.${extension}`;
  const installerPath = path.join(outDir, installerName);
  const workDir = fs.mkdtempSync(path.join(os.tmpdir(), `mcpace-installer-${target.key}-`));
  fs.mkdirSync(outDir, { recursive: true });
  fs.rmSync(installerPath, { force: true });
  try {
    let extra = {};
    if (extension === 'deb') {
      createDebianPackage(installerPath, target, binaryPath, version);
      extra = { built: true };
    } else if (extension === 'msi') {
      extra = createWindowsMsi(installerPath, target, binaryPath, version, workDir);
    } else if (extension === 'pkg') {
      extra = createMacPkg(installerPath, target, binaryPath, version, workDir);
    } else {
      throw new Error(`unsupported installer extension: ${extension}`);
    }
    const built = extra.built !== false;
    const stat = built ? fs.statSync(installerPath) : null;
    return {
      schema: 'mcpace.nativeInstallerAssetBuild.v1',
      status: 'pass',
      target: target.key,
      platform: target.platform,
      arch: target.arch,
      rustTarget: target.rustTarget,
      libc: target.libc ?? null,
      binaryName: binaryNameForTarget(target),
      binarySourcePath: path.relative(repoRoot, binaryPath).split(path.sep).join('/'),
      sidecarBinaries: extra.sidecarBinaries ?? [],
      installerKind: installerKind(target),
      installerName,
      installerPath: built ? path.relative(repoRoot, installerPath).split(path.sep).join('/') : null,
      installerBytes: stat ? stat.size : 0,
      sha256: built ? sha256File(installerPath) : null,
      externalBuildSkipped: !built,
      wixSourcePath: extra.wixSourcePath ? path.relative(repoRoot, extra.wixSourcePath).split(path.sep).join('/') : null,
    };
  } finally {
    fs.rmSync(workDir, { recursive: true, force: true });
  }
}

try {
  const targetKey = argValue('--target') ?? argValue('--target-key');
  const binaryArg = argValue('--binary');
  const outDir = path.resolve(argValue('--out-dir', path.join(repoRoot, 'dist', 'github')));
  if (!targetKey) fail('usage: node scripts/build-native-installer-asset.mjs --target <release-target-key> --binary <path> [--out-dir dist/github] [--json]');
  if (!binaryArg) fail('missing --binary <path>');
  const releaseTargets = readJson('release-targets.json');
  const target = validateTarget(releaseTargets, targetKey);
  const version = deriveProjectVersion();
  const binaryPath = path.resolve(binaryArg);
  validateBinaryForTarget(binaryPath, target);
  readRegularFileStableSync(binaryPath, { maxBytes: Number(process.env.MCPACE_NATIVE_BINARY_MAX_BYTES || 128 * 1024 * 1024) });
  const report = buildInstaller(target, binaryPath, outDir, version);
  if (jsonOutput) {
    process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
  } else {
    process.stdout.write(report.installerPath ? `Built ${report.installerPath}\n` : `Prepared ${report.installerName}\n`);
  }
} catch (error) {
  fail(error?.message ?? String(error));
}
