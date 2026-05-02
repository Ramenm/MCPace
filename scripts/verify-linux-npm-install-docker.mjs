#!/usr/bin/env node
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { deriveProjectVersion, repoRoot } from './lib/project-metadata.mjs';
import { cleanChildEnv } from './lib/safe-child-env.mjs';

const DEFAULT_IMAGE_TAG = 'mcpace-verify-linux-npm-install:local';
const DEFAULT_TARGET_KEY = 'linux-x64-gnu';
const DEFAULT_CPUS = '1.0';
const DEFAULT_MEMORY = '1g';
const DEFAULT_PIDS_LIMIT = '256';
const DEFAULT_DOCKER_BUILD_TIMEOUT_MS = 600000;
const DEFAULT_DOCKER_RUN_TIMEOUT_MS = 1200000;
const DOCKER_BUILD_TIMEOUT_MS = parseTimeoutEnv('MCPACE_DOCKER_BUILD_TIMEOUT_MS', DEFAULT_DOCKER_BUILD_TIMEOUT_MS);
const DOCKER_RUN_TIMEOUT_MS = parseTimeoutEnv('MCPACE_DOCKER_RUN_TIMEOUT_MS', DEFAULT_DOCKER_RUN_TIMEOUT_MS);

function parseTimeoutEnv(name, fallback) {
  const parsed = Number.parseInt(process.env[name] || '', 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}


function parseArgs(argv) {
  const parsed = {
    json: false,
    imageTag: DEFAULT_IMAGE_TAG,
    targetKey: DEFAULT_TARGET_KEY,
    cpus: DEFAULT_CPUS,
    memory: DEFAULT_MEMORY,
    pidsLimit: DEFAULT_PIDS_LIMIT,
    keepImage: false
  };

  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--json':
        parsed.json = true;
        break;
      case '--image-tag':
        parsed.imageTag = argv[++index] || DEFAULT_IMAGE_TAG;
        break;
      case '--target-key':
        parsed.targetKey = argv[++index] || DEFAULT_TARGET_KEY;
        break;
      case '--cpus':
        parsed.cpus = argv[++index] || DEFAULT_CPUS;
        break;
      case '--memory':
        parsed.memory = argv[++index] || DEFAULT_MEMORY;
        break;
      case '--pids-limit':
        parsed.pidsLimit = argv[++index] || DEFAULT_PIDS_LIMIT;
        break;
      case '--keep-image':
        parsed.keepImage = true;
        break;
      default:
        throw new Error(`unsupported verify-linux-npm-install-docker argument: ${token}`);
    }
  }

  return parsed;
}

function stageWorkspace() {
  const stagingRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-linux-npm-install-'));
  for (const entry of [
    'Cargo.toml',
    'Cargo.lock',
    'LICENSE',
    'package.json',
    'release-targets.json',
    'src',
    'scripts',
    'packages'
  ]) {
    fs.cpSync(path.join(repoRoot, entry), path.join(stagingRoot, entry), { recursive: true });
  }
  fs.copyFileSync(path.join(repoRoot, 'scripts', 'verify-image.Dockerfile'), path.join(stagingRoot, 'Dockerfile.verify'));
  return stagingRoot;
}

function runChecked(command, args, options = {}) {
  const result = spawnSync(command, args, {
    encoding: 'utf8',
    env: cleanChildEnv(),
    windowsHide: true,
    ...options
  });
  if (result.status !== 0) {
    const details = [
      result.error?.code === 'ETIMEDOUT'
        ? `${command} timed out after ${options.timeout}ms`
        : result.error?.message,
      result.stdout?.trim() ? `stdout:\n${result.stdout.trim()}` : '',
      result.stderr?.trim() ? `stderr:\n${result.stderr.trim()}` : ''
    ]
      .filter(Boolean)
      .join('\n\n');

    throw new Error(details || `${command} failed with exit code ${result.status}`);
  }
  return result;
}

function removeImage(imageTag) {
  spawnSync('docker', ['rmi', imageTag], {
    encoding: 'utf8',
    env: cleanChildEnv(),
    windowsHide: true,
    timeout: 120000
  });
}

function buildAndRun(options) {
  const expectedVersion = deriveProjectVersion();
  const stagingRoot = stageWorkspace();
  const startedAt = Date.now();
  const imageTag = options.imageTag || DEFAULT_IMAGE_TAG;
  const targetKey = options.targetKey || DEFAULT_TARGET_KEY;

  try {
    runChecked('docker', ['build', '-f', 'Dockerfile.verify', '-t', imageTag, '.'], {
      cwd: stagingRoot,
      timeout: DOCKER_BUILD_TIMEOUT_MS
    });

    const shellScript = `
set -eu
export PATH="/usr/local/cargo/bin:$PATH"
TARGET_KEY="${targetKey}"
EXPECTED_VERSION="${expectedVersion}"
PACK_DIR="/tmp/mcpace-packs"
CONSUMER_DIR="/tmp/mcpace-consumer"
mkdir -p "$PACK_DIR" "$CONSUMER_DIR"
printf '== build linux release binary ==\\n'
cargo build --release >/tmp/mcpace-cargo-build.log 2>&1
printf '== sync platform package metadata ==\\n'
node scripts/sync-platform-packages.mjs --json >/tmp/mcpace-sync-platform.json
printf '== stage linux platform binary ==\\n'
node scripts/stage-platform-package-binary.mjs --json --target-key "$TARGET_KEY" --binary-path target/release/mcpace --clear-bin-dir >/tmp/mcpace-stage-platform.json
printf '== verify platform package contains binary ==\\n'
node scripts/verify-platform-packages.mjs --json --target-key "$TARGET_KEY" --require-binaries >/tmp/mcpace-verify-platform.json
printf '== pack local tarballs ==\\n'
npm pack "packages/npm/cli-$TARGET_KEY" --json --pack-destination "$PACK_DIR" >/tmp/mcpace-platform-pack.json
npm pack packages/npm/cli --json --pack-destination "$PACK_DIR" >/tmp/mcpace-main-pack.json
PLATFORM_TGZ=$(node -e "const fs=require('fs'); const data=JSON.parse(fs.readFileSync('/tmp/mcpace-platform-pack.json','utf8')); process.stdout.write('/tmp/mcpace-packs/'+data[0].filename);")
MAIN_TGZ=$(node -e "const fs=require('fs'); const data=JSON.parse(fs.readFileSync('/tmp/mcpace-main-pack.json','utf8')); process.stdout.write('/tmp/mcpace-packs/'+data[0].filename);")
test -f "$PLATFORM_TGZ"
test -f "$MAIN_TGZ"
printf '== install into clean consumer ==\\n'
cd "$CONSUMER_DIR"
npm init -y >/tmp/mcpace-consumer-init.log 2>&1
npm install --ignore-scripts --no-audit --no-fund "$PLATFORM_TGZ" "$MAIN_TGZ" >/tmp/mcpace-consumer-install.log 2>&1
printf '== run installed launcher ==\\n'
./node_modules/.bin/mcpace version > /tmp/mcpace-installed-version.txt
grep -Eq "^$EXPECTED_VERSION$" /tmp/mcpace-installed-version.txt
node -e "const fs=require('fs'); const version=fs.readFileSync('/tmp/mcpace-installed-version.txt','utf8').trim(); const pkg=require('./node_modules/@mcpace/cli/package.json'); const platform=require('./node_modules/@mcpace/cli-linux-x64-gnu/package.json'); console.log(JSON.stringify({version, mainPackage: pkg.name, platformPackage: platform.name, platformVersion: platform.version}));" > /tmp/mcpace-install-summary.json
cat /tmp/mcpace-install-summary.json
`;

    const runResult = runChecked(
      'docker',
      [
        'run',
        '--rm',
        '--cpus',
        options.cpus || DEFAULT_CPUS,
        '--memory',
        options.memory || DEFAULT_MEMORY,
        '--pids-limit',
        options.pidsLimit || DEFAULT_PIDS_LIMIT,
        '-v',
        `${stagingRoot}:/work`,
        '-w',
        '/work',
        imageTag,
        'sh',
        '-lc',
        shellScript
      ],
      { cwd: repoRoot, timeout: DOCKER_RUN_TIMEOUT_MS }
    );

    let installSummary = null;
    const lastJsonLine = (runResult.stdout || '')
      .trim()
      .split(/\r?\n/)
      .reverse()
      .find((line) => line.trim().startsWith('{'));
    if (lastJsonLine) {
      installSummary = JSON.parse(lastJsonLine);
    }

    return {
      status: 'pass',
      imageTag,
      targetKey,
      cpus: options.cpus || DEFAULT_CPUS,
      memory: options.memory || DEFAULT_MEMORY,
      pidsLimit: options.pidsLimit || DEFAULT_PIDS_LIMIT,
      durationMs: Date.now() - startedAt,
      buildTimeoutMs: DOCKER_BUILD_TIMEOUT_MS,
      runTimeoutMs: DOCKER_RUN_TIMEOUT_MS,
      installSummary
    };
  } finally {
    fs.rmSync(stagingRoot, { recursive: true, force: true });
    if (!options.keepImage) {
      removeImage(imageTag);
    }
  }
}

function main() {
  try {
    const parsed = parseArgs(process.argv.slice(2));
    const report = buildAndRun(parsed);
    if (parsed.json) {
      process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
      return;
    }
    process.stdout.write(`linux npm install docker verification passed in ${report.durationMs}ms\n`);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exit(1);
  }
}

main();
