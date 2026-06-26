import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import zlib from 'node:zlib';
import test from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

function escapeRegExp(value) {
  return String(value).replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

test('native npm package builder exists as the missing final publish lane', () => {
  const script = read('scripts/build-native-npm-package.mjs');
  assert.match(script, /mcpace\.nativeNpmPackageBuild\.v1/);
  assert.match(script, /validateBinaryForTarget/);
  assert.match(script, /copyRegularFileNoFollowSync/);
  assert.match(script, /npm', \['pack'/);
  assert.match(script, /mcpace:\s*\{/);
  assert.match(script, /rustTarget/);
  assert.match(script, /NOTICE/);

  const result = spawnSync(process.execPath, ['scripts/build-native-npm-package.mjs', '--json'], {
    cwd: repoRoot,
    encoding: 'utf8',
    windowsHide: true,
  });
  assert.notEqual(result.status, 0, 'builder must fail closed without explicit target and binary');
  const report = JSON.parse(result.stdout);
  assert.equal(report.status, 'failed');
});

function createBinaryFixture(tmp, name, contents = 'fixture binary\n') {
  const binaryPath = path.join(tmp, name);
  fs.writeFileSync(binaryPath, contents);
  try {
    fs.chmodSync(binaryPath, 0o755);
  } catch {
    // Windows filesystems may ignore POSIX execute bits.
  }
  return binaryPath;
}

function trimNullPaddedAscii(buffer, start, length) {
  return buffer
    .subarray(start, start + length)
    .toString('utf8')
    .replace(/\0.*$/s, '')
    .trim();
}

function tarOctal(buffer, start, length) {
  const raw = trimNullPaddedAscii(buffer, start, length).replace(/\s/g, '');
  return raw ? Number.parseInt(raw, 8) : 0;
}

function listTarGzEntries(data) {
  const buffer = zlib.gunzipSync(data);
  const entries = [];
  let offset = 0;
  while (offset + 512 <= buffer.length) {
    const header = buffer.subarray(offset, offset + 512);
    if (header.every((byte) => byte === 0)) break;
    const name = trimNullPaddedAscii(header, 0, 100);
    const prefix = trimNullPaddedAscii(header, 345, 155);
    const fullName = prefix ? `${prefix}/${name}` : name;
    const mode = tarOctal(header, 100, 8);
    const size = tarOctal(header, 124, 12);
    entries.push({ path: fullName, mode, size });
    offset += 512 + Math.ceil(size / 512) * 512;
  }
  return entries;
}

function readArMembers(filePath) {
  const buffer = fs.readFileSync(filePath);
  assert.equal(buffer.subarray(0, 8).toString('ascii'), '!<arch>\n');
  const members = [];
  let offset = 8;
  while (offset + 60 <= buffer.length) {
    const header = buffer.subarray(offset, offset + 60);
    const name = header.subarray(0, 16).toString('ascii').trim().replace(/\/$/, '');
    const size = Number.parseInt(header.subarray(48, 58).toString('ascii').trim(), 10);
    assert.equal(header.subarray(58, 60).toString('ascii'), '`\n');
    const dataStart = offset + 60;
    const data = buffer.subarray(dataStart, dataStart + size);
    members.push({ name, data });
    offset = dataStart + size + (size % 2);
  }
  return members;
}

test('native GitHub installer builder emits installable package shapes', () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-native-installer-'));
  const outDir = path.join(tmp, 'out');
  try {
    const winBinary = createBinaryFixture(tmp, 'mcpace.exe', 'MZ fixture\n');
    const win = spawnSync(process.execPath, [
      'scripts/build-native-installer-asset.mjs',
      '--target', 'win32-x64-msvc',
      '--binary', winBinary,
      '--out-dir', outDir,
      '--wix-source-only',
      '--json',
    ], {
      cwd: repoRoot,
      encoding: 'utf8',
      windowsHide: true,
    });
    assert.equal(win.status, 0, win.stderr || win.stdout);
    const winReport = JSON.parse(win.stdout);
    assert.equal(winReport.status, 'pass');
    assert.equal(winReport.installerKind, 'windows-msi');
    assert.match(winReport.installerName, /^mcpace-v.+-win32-x64-msvc\.msi$/);
    assert.equal(winReport.externalBuildSkipped, true);
    const wxs = fs.readFileSync(path.join(repoRoot, winReport.wixSourcePath), 'utf8');
    assert.match(wxs, /<Package Name="MCPace"/);
    assert.match(wxs, /<MajorUpgrade /);
    assert.match(wxs, /<Environment Id="MCPacePath" Name="PATH"/);
    assert.match(wxs, /Name="mcpace\.exe"/);
    assert.match(wxs, /MCPaceNotice/);
    assert.match(wxs, /Name="NOTICE"/);

    const linuxBinary = createBinaryFixture(tmp, 'mcpace');
    const linux = spawnSync(process.execPath, [
      'scripts/build-native-installer-asset.mjs',
      '--target', 'linux-x64-gnu',
      '--binary', linuxBinary,
      '--out-dir', outDir,
      '--json',
    ], {
      cwd: repoRoot,
      encoding: 'utf8',
      windowsHide: true,
    });
    assert.equal(linux.status, 0, linux.stderr || linux.stdout);
    const linuxReport = JSON.parse(linux.stdout);
    assert.equal(linuxReport.status, 'pass');
    assert.equal(linuxReport.installerKind, 'debian-package');
    assert.match(linuxReport.installerName, /^mcpace-v.+-linux-x64-gnu\.deb$/);
    assert.match(linuxReport.sha256, /^[a-f0-9]{64}$/);
    const members = readArMembers(path.join(repoRoot, linuxReport.installerPath));
    assert.deepEqual(members.map((member) => member.name), ['debian-binary', 'control.tar.gz', 'data.tar.gz']);
    assert.equal(members[0].data.toString('utf8'), '2.0\n');
    const control = zlib.gunzipSync(members.find((member) => member.name === 'control.tar.gz').data).toString('utf8');
    assert.match(control, /Package: mcpace/);
    assert.match(control, /Architecture: amd64/);
    const dataEntries = listTarGzEntries(members.find((member) => member.name === 'data.tar.gz').data);
    const binaryEntry = dataEntries.find((entry) => entry.path === 'usr/bin/mcpace');
    assert.ok(binaryEntry, 'debian package missing /usr/bin/mcpace');
    assert.notEqual(binaryEntry.mode & 0o111, 0, 'installed linux binary must be executable');
    assert.ok(dataEntries.some((entry) => entry.path === 'usr/share/doc/mcpace/LICENSE'));
    assert.ok(dataEntries.some((entry) => entry.path === 'usr/share/doc/mcpace/NOTICE'));
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test('release index writes checksums and machine-readable update metadata for every enabled installer', () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-release-index-'));
  try {
    const releaseTargets = JSON.parse(read('release-targets.json'));
    const enabledTargets = releaseTargets.targets.filter((entry) => entry.publishEnabled !== false);
    for (const target of enabledTargets) {
      const extension = target.platform === 'win32' ? 'msi' : target.platform === 'darwin' ? 'pkg' : 'deb';
      fs.writeFileSync(path.join(tmp, `mcpace-v0.0.0-${target.key}.${extension}`), `${target.key}\n`);
    }
    const result = spawnSync(process.execPath, [
      'scripts/build-release-index.mjs',
      '--asset-dir', tmp,
      '--out-dir', tmp,
      '--json',
    ], {
      cwd: repoRoot,
      encoding: 'utf8',
      windowsHide: true,
    });
    assert.equal(result.status, 0, result.stderr || result.stdout);
    const report = JSON.parse(result.stdout);
    assert.equal(report.status, 'pass');
    assert.equal(report.missingNativeInstallerTargets.length, 0);
    assert.equal(report.assets.filter((asset) => asset.target).length, enabledTargets.length);
    const manifest = JSON.parse(fs.readFileSync(path.join(repoRoot, report.manifestPath), 'utf8'));
    assert.equal(manifest.updatePolicy.mode, 'package-manager-managed');
    assert.equal(manifest.updatePolicy.directGitHubInstallers, 'manual-upgrade-only');
    assert.equal(manifest.updatePolicy.selfRewrite, false);
    assert.match(manifest.updatePolicy.futureOsUpdateChannels.linux, /signed apt repository/);
    assert.match(manifest.updatePolicy.futureOsUpdateChannels.windows, /winget/);
    assert.match(manifest.updatePolicy.futureOsUpdateChannels.macos, /notarized PKG/);
    assert.equal(manifest.ubuntu.packageFormat, 'deb');
    assert.equal(manifest.ubuntu.glibcBaselineImage, 'ubuntu:22.04');
    assert.equal(manifest.ubuntu.x64, 'linux-x64-gnu');
    assert.equal(manifest.ubuntu.arm64, 'linux-arm64-gnu');
    assert.ok(manifest.assets.some((asset) => asset.name.endsWith('.msi') && asset.target.installerKind === 'windows-msi'));
    assert.ok(manifest.assets.some((asset) => asset.name.endsWith('.deb') && asset.target.installCommand.includes('apt install')));
    assert.ok(manifest.assets.some((asset) => asset.name.endsWith('.deb') && asset.target.glibcBaselineImage === 'ubuntu:22.04'));
    const checksums = fs.readFileSync(path.join(repoRoot, report.checksumsPath), 'utf8');
    assert.match(checksums, /mcpace-v0\.0\.0-linux-x64-gnu\.deb/);
    assert.match(checksums, /mcpace-v0\.0\.0-win32-x64-msvc\.msi/);
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test('release index fails closed when any enabled installer is missing', () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-release-index-missing-'));
  try {
    const releaseTargets = JSON.parse(read('release-targets.json'));
    const enabledTargets = releaseTargets.targets.filter((entry) => entry.publishEnabled !== false);
    const [omittedTarget, ...includedTargets] = enabledTargets;
    for (const target of includedTargets) {
      const extension = target.platform === 'win32' ? 'msi' : target.platform === 'darwin' ? 'pkg' : 'deb';
      fs.writeFileSync(path.join(tmp, `mcpace-v0.0.0-${target.key}.${extension}`), `${target.key}\n`);
    }
    const result = spawnSync(process.execPath, [
      'scripts/build-release-index.mjs',
      '--asset-dir', tmp,
      '--out-dir', tmp,
      '--json',
    ], {
      cwd: repoRoot,
      encoding: 'utf8',
      windowsHide: true,
    });
    assert.notEqual(result.status, 0, 'release index must fail when an enabled installer is missing');
    const report = JSON.parse(result.stdout);
    assert.equal(report.status, 'failed');
    assert.match(report.error, new RegExp(`missing native GitHub release installers for targets: ${escapeRegExp(omittedTarget.key)}`));
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test('publish workflow builds native packages before publishing the main launcher', () => {
  const workflow = read('.github/workflows/publish-npm.yml');
  assert.match(workflow, /native-packages:/);
  assert.match(workflow, /needs:\s*native-packages/);
  assert.match(workflow, /cargo fmt --check/);
  assert.match(workflow, /cargo clippy --all-targets --target/);
  assert.match(workflow, /cargo test --target/);
  assert.match(workflow, /Build and test Linux native binary in glibc baseline container/);
  assert.match(workflow, /bash scripts\/build-linux-glibc-baseline\.sh/);
  assert.match(workflow, /MCPACE_GLIBC_BASELINE_CHECKS: full/);
  assert.match(workflow, /glibc_baseline_image: ubuntu:22\.04/);
  assert.match(workflow, /node scripts\/build-native-npm-package\.mjs --target/);
  assert.match(workflow, /tarballs=\(dist\/npm\/\*\.tgz\)/);
  assert.match(workflow, /expected 6 native npm tarballs/);
  assert.match(workflow, /node scripts\/verify-npm-publish-contract\.mjs --enforce/);
  assert.ok(workflow.indexOf('Publish native npm packages') < workflow.indexOf('Publish main npm launcher'));
});

test('release workflow builds platform installers and checksummed draft GitHub release assets', () => {
  const workflow = read('.github/workflows/release.yml');
  assert.match(workflow, /native-release-assets:/);
  assert.match(workflow, /native GitHub installer/);
  assert.match(workflow, /compose-release-assets:/);
  assert.match(workflow, /dotnet tool install --global wix/);
  assert.match(workflow, /Build Linux native binary in glibc baseline container/);
  assert.match(workflow, /bash scripts\/build-linux-glibc-baseline\.sh/);
  assert.match(workflow, /MCPACE_GLIBC_BASELINE_CHECKS: build/);
  assert.match(workflow, /glibc_baseline_image: ubuntu:22\.04/);
  assert.match(workflow, /node scripts\/build-native-installer-asset\.mjs --target/);
  assert.match(workflow, /pattern: native-release-\*/);
  assert.match(workflow, /Verify Ubuntu\/Debian installer/);
  assert.match(workflow, /Verify Windows MSI installer/);
  assert.match(workflow, /PassThru/);
  assert.match(workflow, /msiexec failed with exit code/);
  assert.match(workflow, /Verify macOS PKG installer/);
  assert.match(workflow, /npm run platform:binary-smoke -- --binary/);
  assert.match(workflow, /actions\/attest@v4/);
  assert.match(workflow, /node scripts\/build-release-index\.mjs --json --asset-dir dist\/release-assets --out-dir dist\/release-assets/);
  assert.match(workflow, /name: github-release-assets/);
  assert.match(workflow, /mcpace-v.+-checksums\.sha256|checksums and release asset manifest/);
  assert.match(workflow, /gh release upload "\$RELEASE_TAG" "\$\{assets\[@\]\}" --clobber/);
});

test('release workflow native asset matrix mirrors release target metadata', () => {
  const workflow = read('.github/workflows/release.yml');
  const releaseTargets = JSON.parse(read('release-targets.json'));
  const enabledTargets = releaseTargets.targets.filter((entry) => entry.publishEnabled !== false);
  const plannedTargets = releaseTargets.plannedTargets.filter((entry) => entry.publishEnabled === false);
  for (const target of enabledTargets) {
    assert.match(workflow, new RegExp(`- key: ${escapeRegExp(target.key)}\n`));
    assert.match(workflow, new RegExp(`platform: ${escapeRegExp(target.platform)}`));
    assert.match(workflow, new RegExp(`rust_target: ${escapeRegExp(target.rustTarget)}`));
    assert.match(workflow, new RegExp(`binary_name: ${escapeRegExp(target.binaryName)}`));
    if (target.glibcBaselineImage) {
      assert.match(workflow, new RegExp(`glibc_baseline_image: ${escapeRegExp(target.glibcBaselineImage)}`));
    }
  }
  for (const target of plannedTargets) {
    assert.doesNotMatch(workflow, new RegExp(`- key: ${escapeRegExp(target.key)}\n`), `${target.key} must stay out of native release matrix until enabled`);
  }
});

test('publish workflow native matrix mirrors release target metadata', () => {
  const workflow = read('.github/workflows/publish-npm.yml');
  const releaseTargets = JSON.parse(read('release-targets.json'));
  const enabledTargets = releaseTargets.targets.filter((entry) => entry.publishEnabled !== false);
  for (const target of enabledTargets) {
    assert.match(workflow, new RegExp(`- key: ${escapeRegExp(target.key)}\n`));
    assert.match(workflow, new RegExp(`platform: ${escapeRegExp(target.platform)}`));
    assert.match(workflow, new RegExp(`rust_target: ${escapeRegExp(target.rustTarget)}`));
    assert.match(workflow, new RegExp(`binary_name: ${escapeRegExp(target.binaryName)}`));
    if (target.glibcBaselineImage) {
      assert.match(workflow, new RegExp(`glibc_baseline_image: ${escapeRegExp(target.glibcBaselineImage)}`));
    }
  }
});

test('release completion documentation and scripts are part of the source release', () => {
  const manifest = JSON.parse(read('release-manifest.json'));
  assert.ok(manifest.includePaths.includes('docs/release-completion.md'));
  assert.ok(manifest.includePaths.includes('docs/signing-and-notarization.md'));
  assert.ok(manifest.includePaths.includes('NOTICE'));
  assert.ok(manifest.includePaths.includes('scripts/build-native-npm-package.mjs'));
  assert.ok(manifest.includePaths.includes('scripts/build-native-installer-asset.mjs'));
  assert.ok(manifest.includePaths.includes('scripts/build-linux-glibc-baseline.sh'));
  assert.ok(manifest.includePaths.includes('scripts/build-release-index.mjs'));
  assert.ok(!manifest.includePaths.includes('scripts/build-native-release-asset.mjs'));
  const docs = read('docs/release-completion.md');
  assert.match(docs, /A tarball that merely has the right filename is not enough/);
  assert.match(docs, /npm trusted publishers are configured/);
  assert.match(docs, /source-code archives/);
  assert.match(docs, /Windows users install/);
  assert.match(docs, /Ubuntu users install/);
  assert.match(docs, /manual-upgrade-only/);
  assert.match(docs, /signed apt repository/);
  assert.match(docs, /Authenticode-sign/);
  assert.match(docs, /notarize with Apple/);
  assert.match(docs, /docs\/signing-and-notarization\.md/);
  assert.match(docs, /full-length commit SHAs/);
  assert.match(docs, /Third-party notices/);
  assert.match(docs, /Apache-2\.0/);
  assert.match(docs, /Copyright 2026 Ramenm/);
  assert.match(docs, /ubuntu:22\.04/);
  assert.match(docs, /Ubuntu 24\.04/);
  const rootReadme = read('README.md');
  assert.match(rootReadme, /npm install -g @mcpace\/cli@latest/);
  assert.match(rootReadme, /Copyright 2026 Ramenm/);
  const npmReadme = read('packages/npm/cli/README.md');
  assert.match(npmReadme, /npm install -g @mcpace\/cli@latest/);
  assert.match(npmReadme, /Apache-2\.0/);
  const signingRunbook = read('docs/signing-and-notarization.md');
  assert.match(signingRunbook, /Microsoft Artifact Signing/);
  assert.match(signingRunbook, /GitHub Actions OIDC/);
  assert.match(signingRunbook, /Developer ID Application/);
  assert.match(signingRunbook, /Developer ID Installer/);
  assert.match(signingRunbook, /notarytool submit --wait/);
  assert.match(signingRunbook, /stapler staple/);
  assert.match(signingRunbook, /Get-AuthenticodeSignature/);
  assert.match(signingRunbook, /windows-11-arm/);
});
