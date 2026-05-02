#!/usr/bin/env node
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { deriveProjectVersion, repoRoot } from './lib/project-metadata.mjs';
import { cleanChildEnv } from './lib/safe-child-env.mjs';
const DEFAULT_IMAGE = 'rust:1.95-bookworm';
const DEFAULT_CPUS = '1.0';
const DEFAULT_MEMORY = '768m';
const DEFAULT_PIDS_LIMIT = '256';
const DEFAULT_DOCKER_TIMEOUT_MS = 900000;
const DOCKER_TIMEOUT_MS = parseTimeoutEnv('MCPACE_DOCKER_TIMEOUT_MS', DEFAULT_DOCKER_TIMEOUT_MS);

function parseTimeoutEnv(name, fallback) {
  const parsed = Number.parseInt(process.env[name] || '', 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}


function parseArgs(argv) {
  const parsed = {
    image: DEFAULT_IMAGE,
    json: false,
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
      case '--image':
        parsed.image = argv[++index] || DEFAULT_IMAGE;
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
        throw new Error(`unsupported verify-ubuntu-docker-e2e argument: ${token}`);
    }
  }

  return parsed;
}

function stageWorkspace() {
  const stagingRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-ubuntu-e2e-'));
  for (const entry of ['Cargo.toml', 'Cargo.lock', 'src']) {
    const source = path.join(repoRoot, entry);
    const destination = path.join(stagingRoot, entry);
    fs.cpSync(source, destination, { recursive: true });
  }
  return stagingRoot;
}

function runDockerCheck({ image, cpus, memory, pidsLimit }) {
  const configVersion = deriveProjectVersion();
  const stagingRoot = stageWorkspace();
  const startedAt = Date.now();
  try {
    const shellScript = `
set -eu
export PATH="/usr/local/cargo/bin:$PATH"
cargo build >/tmp/mcpace-build.log 2>&1
ROOT="$(mktemp -d)"
cleanup() {
  ./target/debug/mcpace hub down --json --root "$ROOT" >/tmp/mcpace-down.log 2>&1 || true
  rm -rf "$ROOT"
}
trap cleanup EXIT
cat >"$ROOT/mcpace.config.json" <<'EOF'
{
  "version": "${configVersion}",
  "profiles": {
    "runtime": {
      "default": "safe",
      "profiles": {
        "safe": { "description": "Safe", "serverOverrides": {} }
      }
    }
  },
  "servers": {}
}
EOF
mkdir -p "$ROOT/data/runtime/hub"
printf '{ not-valid-json' >"$ROOT/data/runtime/hub/state.json"
./target/debug/mcpace hub status --json --root "$ROOT" >/tmp/mcpace-status-corrupt.json 2>/tmp/mcpace-status-corrupt.err
grep -Eq '"status": "corrupt"' /tmp/mcpace-status-corrupt.json
./target/debug/mcpace hub repair --json --root "$ROOT" >/tmp/mcpace-repair.json 2>/tmp/mcpace-repair.err
grep -Eq '"status": "stopped"' /tmp/mcpace-repair.json
./target/debug/mcpace hub up --json --root "$ROOT" >/tmp/mcpace-up.json 2>/tmp/mcpace-up.err
grep -Eq '"status": "(running|starting)"' /tmp/mcpace-up.json
./target/debug/mcpace hub logs --json --root "$ROOT" --tail 20 >/tmp/mcpace-logs.json 2>/tmp/mcpace-logs.err
grep -Eq 'hub_started|hub_starting' /tmp/mcpace-logs.json
./target/debug/mcpace hub down --json --root "$ROOT" >/tmp/mcpace-down.json 2>/tmp/mcpace-down.err
grep -Eq '"status": "stopped"' /tmp/mcpace-down.json
cat /tmp/mcpace-status-corrupt.json
printf '\\n'
cat /tmp/mcpace-repair.json
printf '\\n'
cat /tmp/mcpace-up.json
printf '\\n'
cat /tmp/mcpace-down.json
`;

    const result = spawnSync(
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
        image,
        'sh',
        '-lc',
        shellScript
      ],
      {
        encoding: 'utf8',
        cwd: repoRoot,
        env: cleanChildEnv(),
        timeout: DOCKER_TIMEOUT_MS,
        windowsHide: true
      }
    );

    return {
      image,
      cpus,
      memory,
      pidsLimit,
      durationMs: Date.now() - startedAt,
      timeoutMs: DOCKER_TIMEOUT_MS,
      timedOut: result.error?.code === 'ETIMEDOUT',
      status: result.status,
      stdout: result.stdout || '',
      stderr: result.stderr || '',
      error: result.error?.code === 'ETIMEDOUT'
        ? `docker verification timed out after ${DOCKER_TIMEOUT_MS}ms`
        : result.error
          ? result.error.message
          : null
    };
  } finally {
    fs.rmSync(stagingRoot, { recursive: true, force: true });
  }
}

function main() {
  try {
    const parsed = parseArgs(process.argv.slice(2));
    const report = runDockerCheck(parsed);
    if (report.status !== 0) {
      throw new Error(
        report.error ||
          report.stderr.trim() ||
          report.stdout.trim() ||
          'ubuntu docker e2e verification failed'
      );
    }

    if (parsed.json) {
      process.stdout.write(
        `${JSON.stringify(
          {
            image: report.image,
            cpus: report.cpus,
            memory: report.memory,
            pidsLimit: report.pidsLimit,
            durationMs: report.durationMs,
            timeoutMs: report.timeoutMs,
            timedOut: report.timedOut,
            status: report.status
          },
          null,
          2
        )}\n`
      );
      return;
    }

    process.stdout.write(
      `ubuntu-docker-e2e verification passed in ${report.durationMs}ms using ${report.image}\n`
    );
    process.stdout.write(report.stdout);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exit(1);
  }
}

main();
