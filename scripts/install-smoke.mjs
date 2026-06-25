#!/usr/bin/env node
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { repoRoot } from './lib/project-metadata.mjs';
import { commandForPlatform, runChecked } from './lib/process.mjs';

const args = process.argv.slice(2);
const jsonOutput = args.includes('--json');
const keepTemp = args.includes('--keep-temp');

function writeJson(payload) {
  process.stdout.write(`${JSON.stringify(payload, null, 2)}\n`);
}

function binPathForInstall(appDir, commandName = 'mcpace') {
  const binName = process.platform === 'win32' ? `${commandName}.cmd` : commandName;
  return path.join(appDir, 'node_modules', '.bin', binName);
}

function createNativeFixture(tmpDir) {
  const native = path.join(tmpDir, process.platform === 'win32' ? 'mcpace-native-fixture.cmd' : 'mcpace-native-fixture');
  const body = process.platform === 'win32'
    ? `@echo off\r\nnode -e "require('fs').writeFileSync(process.env.MCPACE_INSTALL_SMOKE_OUT, JSON.stringify(process.argv.slice(1)))" %*\r\n`
    : `#!/usr/bin/env node\nimport fs from 'node:fs';\nfs.writeFileSync(process.env.MCPACE_INSTALL_SMOKE_OUT, JSON.stringify(process.argv.slice(2)));\n`;
  fs.writeFileSync(native, body, 'utf8');
  if (process.platform !== 'win32') fs.chmodSync(native, 0o755);
  return native;
}

function parsePackOutput(stdout) {
  const parsed = JSON.parse(stdout);
  if (!Array.isArray(parsed) || parsed.length !== 1 || !parsed[0]?.filename) {
    throw new Error(`unexpected npm pack output: ${stdout}`);
  }
  return parsed[0];
}

function runInstallSmoke() {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-install-smoke-'));
  const appDir = path.join(tmpDir, 'app');
  const pkgDir = path.join(tmpDir, 'pkg');
  fs.mkdirSync(appDir, { recursive: true });
  fs.mkdirSync(pkgDir, { recursive: true });

  try {
    fs.writeFileSync(path.join(appDir, 'package.json'), '{"private":true}\n', 'utf8');

    const pack = runChecked('npm', ['pack', '--workspace', '@mcpace/cli', '--json', '--pack-destination', pkgDir], {
      cwd: repoRoot,
      encoding: 'utf8',
      maxBuffer: 32 * 1024 * 1024,
    });
    const packed = parsePackOutput(pack.stdout);
    const tarballPath = path.join(pkgDir, packed.filename);
    if (!fs.existsSync(tarballPath)) {
      throw new Error(`npm pack reported ${packed.filename}, but the tarball was not created`);
    }

    runChecked('npm', ['install', '--ignore-scripts', '--no-audit', '--no-fund', '--omit=optional', '--registry', 'http://127.0.0.1:9', '--fetch-retries=0', '--fetch-timeout=1000', '--prefix', appDir, tarballPath], {
      cwd: repoRoot,
      encoding: 'utf8',
      maxBuffer: 32 * 1024 * 1024,
    });

    const native = createNativeFixture(tmpDir);
    const markerArgs = ['doctor', '--json', 'semi;no-shell', 'space arg'];
    const observed = {};
    const installedBins = {};
    const commandStatuses = {};

    for (const commandName of ['mcpace']) {
      const commandOutPath = path.join(tmpDir, `${commandName}-argv.json`);
      const installedBin = binPathForInstall(appDir, commandName);
      if (!fs.existsSync(installedBin)) {
        throw new Error(`installed npm bin is missing: ${installedBin}`);
      }

      const env = {
        ...process.env,
        MCPACE_BINARY_PATH: native,
        MCPACE_INSTALL_SMOKE_OUT: commandOutPath,
      };
      const result = runChecked(commandForPlatform(installedBin), markerArgs, {
        cwd: appDir,
        encoding: 'utf8',
        env,
        maxBuffer: 32 * 1024 * 1024,
      });
      const observedArgs = JSON.parse(fs.readFileSync(commandOutPath, 'utf8'));
      if (JSON.stringify(observedArgs) !== JSON.stringify(markerArgs)) {
        throw new Error(`${commandName} installed bin argument mismatch: expected ${JSON.stringify(markerArgs)}, got ${JSON.stringify(observedArgs)}`);
      }
      observed[commandName] = observedArgs;
      installedBins[commandName] = path.relative(tmpDir, installedBin).split(path.sep).join('/');
      commandStatuses[commandName] = result.status;
    }

    return {
      schema: 'mcpace.installSmoke.v1',
      status: 'pass',
      package: '@mcpace/cli',
      tarball: packed.filename,
      installedBins,
      nativeFixture: path.basename(native),
      observedArgs: observed,
      tempDir: keepTemp ? tmpDir : null,
      commandStatuses,
    };
  } finally {
    if (!keepTemp) fs.rmSync(tmpDir, { recursive: true, force: true });
  }
}

try {
  const payload = runInstallSmoke();
  if (jsonOutput) writeJson(payload);
  else process.stdout.write(`PASS install smoke: ${payload.tarball} (${Object.keys(payload.installedBins).join(', ')})\n`);
} catch (error) {
  const payload = {
    schema: 'mcpace.installSmoke.v1',
    status: 'fail',
    error: error?.message || String(error),
  };
  if (jsonOutput) writeJson(payload);
  else process.stderr.write(`${error?.stack || error}\n`);
  process.exitCode = 1;
}
