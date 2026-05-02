#!/usr/bin/env node
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { cleanChildEnv } from './lib/safe-child-env.mjs';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const DEFAULT_IMAGE = 'rust:1.95-bookworm';
const DEFAULT_TEST = 'hub_up_releases_captured_stdio_for_background_launcher';
const DEFAULT_CPUS = '1.0';
const DEFAULT_MEMORY = '768m';
const DEFAULT_PIDS_LIMIT = '256';
const DEFAULT_DOCKER_TIMEOUT_MS = 600000;
const DOCKER_TIMEOUT_MS = parseTimeoutEnv('MCPACE_DOCKER_TIMEOUT_MS', DEFAULT_DOCKER_TIMEOUT_MS);

function parseTimeoutEnv(name, fallback) {
  const parsed = Number.parseInt(process.env[name] || '', 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}


function parseArgs(argv) {
  const parsed = {
    image: DEFAULT_IMAGE,
    json: false,
    testName: DEFAULT_TEST,
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
      case '--test-name':
        parsed.testName = argv[++index] || DEFAULT_TEST;
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
        throw new Error(`unsupported verify-ubuntu-docker-fast argument: ${token}`);
    }
  }

  return parsed;
}

function stageWorkspace() {
  const stagingRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-ubuntu-fast-'));
  for (const entry of ['Cargo.toml', 'Cargo.lock', 'src', 'tests']) {
    const source = path.join(repoRoot, entry);
    const destination = path.join(stagingRoot, entry);
    fs.cpSync(source, destination, { recursive: true });
  }
  return stagingRoot;
}

function runDockerCheck({ image, testName, cpus, memory, pidsLimit }) {
  const stagingRoot = stageWorkspace();
  const startedAt = Date.now();
  try {
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
        `export PATH="/usr/local/cargo/bin:$PATH"; cargo test --test hub_runtime ${testName} -- --exact`
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
      testName,
      cpus,
      memory,
      pidsLimit,
      stagingRoot,
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
          'ubuntu docker fast verification failed'
      );
    }

    if (parsed.json) {
      process.stdout.write(
        `${JSON.stringify(
          {
            image: report.image,
            testName: report.testName,
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
      `ubuntu-docker-fast verification passed in ${report.durationMs}ms using ${report.image}\n`
    );
    process.stdout.write(report.stdout);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exit(1);
  }
}

main();
