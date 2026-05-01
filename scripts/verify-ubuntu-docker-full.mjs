#!/usr/bin/env node
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { deriveProjectVersion, repoRoot } from './lib/project-metadata.mjs';
const DEFAULT_IMAGE_TAG = 'mcpace-verify:local';
const DEFAULT_CPUS = '1.0';
const DEFAULT_MEMORY = '768m';
const DEFAULT_PIDS_LIMIT = '256';
const DEFAULT_DOCKER_BUILD_TIMEOUT_MS = 600000;
const DEFAULT_DOCKER_RUN_TIMEOUT_MS = 1200000;
const DOCKER_BUILD_TIMEOUT_MS = parseTimeoutEnv(
  'MCPACE_DOCKER_BUILD_TIMEOUT_MS',
  DEFAULT_DOCKER_BUILD_TIMEOUT_MS
);
const DOCKER_RUN_TIMEOUT_MS = parseTimeoutEnv(
  'MCPACE_DOCKER_RUN_TIMEOUT_MS',
  DEFAULT_DOCKER_RUN_TIMEOUT_MS
);

function parseTimeoutEnv(name, fallback) {
  const parsed = Number.parseInt(process.env[name] || '', 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}

function cleanChildEnv() {
  const env = { ...process.env };
  delete env.NODE_TEST_CONTEXT;
  return env;
}

function parseArgs(argv) {
  const parsed = {
    json: false,
    imageTag: DEFAULT_IMAGE_TAG,
    cpus: DEFAULT_CPUS,
    memory: DEFAULT_MEMORY,
    pidsLimit: DEFAULT_PIDS_LIMIT
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
      case '--cpus':
        parsed.cpus = argv[++index] || DEFAULT_CPUS;
        break;
      case '--memory':
        parsed.memory = argv[++index] || DEFAULT_MEMORY;
        break;
      case '--pids-limit':
        parsed.pidsLimit = argv[++index] || DEFAULT_PIDS_LIMIT;
        break;
      default:
        throw new Error(`unsupported verify-ubuntu-docker-full argument: ${token}`);
    }
  }

  return parsed;
}

function stageWorkspace() {
  const stagingRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-ubuntu-full-'));
  for (const entry of [
    'Cargo.toml',
    'Cargo.lock',
    'package.json',
    'release-manifest.json',
    'mcpace.config.json',
    'mcp_settings.json',
    'server-candidates.json',
    'src',
    'packages'
  ]) {
    const source = path.join(repoRoot, entry);
    const destination = path.join(stagingRoot, entry);
    fs.cpSync(source, destination, { recursive: true });
  }

  const dockerfileSource = path.join(repoRoot, 'scripts', 'verify-image.Dockerfile');
  fs.copyFileSync(dockerfileSource, path.join(stagingRoot, 'Dockerfile.verify'));
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

    throw new Error(details || `${command} failed`);
  }
  return result;
}

function buildAndRun({ imageTag, cpus, memory, pidsLimit }) {
  const expectedVersion = deriveProjectVersion();
  const stagingRoot = stageWorkspace();
  const startedAt = Date.now();
  try {
    runChecked(
      'docker',
      ['build', '-f', 'Dockerfile.verify', '-t', imageTag, '.'],
      { cwd: stagingRoot, timeout: DOCKER_BUILD_TIMEOUT_MS }
    );

    const shellScript = `
set -eu
export PATH="/usr/local/cargo/bin:$PATH"
printf '== build release ==\\n'
cargo build --release >/tmp/mcpace-build.log 2>&1
printf '== version ==\\n'
./target/release/mcpace version > /tmp/mcpace-version.txt
grep -Eq '^${expectedVersion.replace(/\./g, '\\.')}$' /tmp/mcpace-version.txt
printf '== doctor ==\\n'
./target/release/mcpace doctor --json > /tmp/mcpace-doctor.json
grep -Eq '"configFound": true' /tmp/mcpace-doctor.json
grep -Eq '"rustSourceReady": true' /tmp/mcpace-doctor.json
grep -Eq '"npmSurfaceReady": true' /tmp/mcpace-doctor.json
printf '== client list ==\\n'
./target/release/mcpace client list --json > /tmp/mcpace-client-list.json
SMOKE_CLIENT_ID=$(node -e "const fs=require('fs'); const data=JSON.parse(fs.readFileSync('/tmp/mcpace-client-list.json','utf8')); const targets=Array.isArray(data.targets)?data.targets:[]; const chosen=targets.find((target)=>target.proofTier==='tier-1' && target.surfaceClass==='local' && target.installSupported===true) || targets.find((target)=>target.surfaceClass==='local' && target.installSupported===true) || targets[0]; if(!chosen){process.exit(1);} process.stdout.write(chosen.id);")
test -n "$SMOKE_CLIENT_ID"
printf '== client plan ==\\n'
./target/release/mcpace client plan --json --client-id "$SMOKE_CLIENT_ID" --session-id docker-e2e --project-root /work > /tmp/mcpace-client-plan.json
grep -Eq '"requiresHubOwnedStdio": true' /tmp/mcpace-client-plan.json
printf '== server list ==\\n'
./target/release/mcpace server list --json > /tmp/mcpace-server-list.json
grep -Eq '"servers": \[\]' /tmp/mcpace-server-list.json
printf '== verify doctor ==\\n'
./target/release/mcpace verify doctor --json > /tmp/mcpace-verify-doctor.json
grep -Eq '"configFound": true' /tmp/mcpace-verify-doctor.json
printf '== verify readiness ==\\n'
./target/release/mcpace verify readiness --json > /tmp/mcpace-verify-readiness.json
grep -Eq '"readyForReadOnlyOps": true' /tmp/mcpace-verify-readiness.json
mkdir -p /work/data/runtime/hub
printf '{ not-valid-json' >/work/data/runtime/hub/state.json
printf '== status (corrupt) ==\\n'
./target/release/mcpace hub status --json > /tmp/mcpace-status-corrupt.json
grep -Eq '"status": "corrupt"' /tmp/mcpace-status-corrupt.json
printf '== top-level repair ==\\n'
./target/release/mcpace repair --json > /tmp/mcpace-repair.json
grep -Eq '"status": "stopped"' /tmp/mcpace-repair.json
printf '== hub up ==\\n'
./target/release/mcpace hub up --json > /tmp/mcpace-up.json
grep -Eq '"status": "(running|starting)"' /tmp/mcpace-up.json
printf '== hub logs ==\\n'
./target/release/mcpace hub logs --json --tail 20 > /tmp/mcpace-logs.json
grep -Eq 'hub_repaired|hub_started|hub_starting' /tmp/mcpace-logs.json
printf '== hub down ==\\n'
./target/release/mcpace hub down --json > /tmp/mcpace-down.json
grep -Eq '"status": "stopped"' /tmp/mcpace-down.json
printf '== final status ==\\n'
./target/release/mcpace hub status --json > /tmp/mcpace-final-status.json
grep -Eq '"status": "stopped"' /tmp/mcpace-final-status.json
cat /tmp/mcpace-doctor.json
printf '\\n'
cat /tmp/mcpace-verify-readiness.json
printf '\\n'
cat /tmp/mcpace-repair.json
printf '\\n'
cat /tmp/mcpace-up.json
printf '\\n'
cat /tmp/mcpace-down.json
`;

    const runResult = runChecked(
      'docker',
      [
        'run',
        '--rm',
        '--cpus',
        cpus,
        '--memory',
        memory,
        '--pids-limit',
        pidsLimit,
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

    return {
      imageTag,
      cpus,
      memory,
      pidsLimit,
      durationMs: Date.now() - startedAt,
      buildTimeoutMs: DOCKER_BUILD_TIMEOUT_MS,
      runTimeoutMs: DOCKER_RUN_TIMEOUT_MS,
      stdout: runResult.stdout || '',
      stderr: runResult.stderr || ''
    };
  } finally {
    fs.rmSync(stagingRoot, { recursive: true, force: true });
  }
}

function main() {
  try {
    const parsed = parseArgs(process.argv.slice(2));
    const report = buildAndRun(parsed);

    if (parsed.json) {
      process.stdout.write(
        `${JSON.stringify(
          {
            imageTag: report.imageTag,
            cpus: report.cpus,
            memory: report.memory,
            pidsLimit: report.pidsLimit,
            durationMs: report.durationMs,
            buildTimeoutMs: report.buildTimeoutMs,
            runTimeoutMs: report.runTimeoutMs,
            status: 0
          },
          null,
          2
        )}\n`
      );
      return;
    }

    process.stdout.write(
      `ubuntu-docker-full verification passed in ${report.durationMs}ms using ${report.imageTag}\n`
    );
    process.stdout.write(report.stdout);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exit(1);
  }
}

main();
