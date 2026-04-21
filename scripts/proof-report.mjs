#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const DEFAULT_OUTPUT_PATH = path.join(repoRoot, 'reports', 'verification-latest.json');
const DEFAULT_ARCHIVE_OUTPUT_DIR = path.join(repoRoot, 'dist');

function readJson(relativePath) {
  return JSON.parse(fs.readFileSync(path.join(repoRoot, relativePath), 'utf8'));
}

function readText(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

function extractTomlVersion(text) {
  const match = text.match(/^version\s*=\s*"([^"]+)"/m);
  return match ? match[1] : null;
}

function firstNonEmptyLine(value) {
  return String(value || '')
    .split(/\r?\n/)
    .map((line) => line.trim())
    .find(Boolean) || null;
}

function normalizePathForReport(filePath) {
  const relative = path.relative(repoRoot, filePath);
  return relative && !relative.startsWith('..') ? relative.split(path.sep).join('/') : filePath;
}

function detectContainerEnvironment() {
  return Boolean(
    process.env.CONTAINER === 'true' ||
      fs.existsSync('/.dockerenv') ||
      fs.existsSync('/run/.containerenv')
  );
}

function detectCommandVersion(command, args = ['--version']) {
  const result = spawnSync(command, args, {
    cwd: repoRoot,
    encoding: 'utf8'
  });

  if (result.error || result.status !== 0) {
    return null;
  }

  return firstNonEmptyLine(result.stdout) || firstNonEmptyLine(result.stderr);
}

function summarizeFailure(result) {
  const combined = [result.stderr, result.stdout]
    .filter(Boolean)
    .join('\n')
    .trim();
  if (!combined) {
    return `exit code ${result.status ?? 'unknown'}`;
  }
  return combined
    .split(/\r?\n/)
    .slice(-12)
    .join('\n');
}

function runCheckedCommand(command, args, label, cwd = repoRoot) {
  const startedAt = Date.now();
  const result = spawnSync(command, args, {
    cwd,
    encoding: 'utf8'
  });
  return {
    label,
    command: [command, ...args].join(' '),
    ok: result.status === 0,
    status: result.status,
    signal: result.signal ?? null,
    durationMs: Date.now() - startedAt,
    stdout: result.stdout || '',
    stderr: result.stderr || '',
    error: result.error ? String(result.error.message || result.error) : null
  };
}

function runArchiveBuilder(outputDir, stamp = null) {
  const args = ['scripts/archive-release.mjs', '--json', '--output-dir', outputDir];
  if (stamp) {
    args.push('--stamp', stamp);
  }

  const result = runCheckedCommand(
    process.execPath,
    args,
    'node scripts/archive-release.mjs --json'
  );

  if (!result.ok) {
    return result;
  }

  try {
    result.archive = JSON.parse(result.stdout);
  } catch (error) {
    result.ok = false;
    result.error = `failed to parse archive builder output: ${error instanceof Error ? error.message : String(error)}`;
  }

  return result;
}

export function parseArgs(argv) {
  const parsed = {
    json: false,
    write: false,
    noRun: false,
    checkedAt: null,
    outputPath: DEFAULT_OUTPUT_PATH,
    archiveOutputDir: DEFAULT_ARCHIVE_OUTPUT_DIR,
    archiveStamp: null
  };

  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--json':
        parsed.json = true;
        break;
      case '--write':
        parsed.write = true;
        break;
      case '--no-run':
        parsed.noRun = true;
        break;
      case '--checked-at':
        parsed.checkedAt = argv[++index] || null;
        break;
      case '--output-path':
        parsed.outputPath = path.resolve(argv[++index] || '');
        break;
      case '--archive-output-dir':
        parsed.archiveOutputDir = path.resolve(argv[++index] || '');
        break;
      case '--archive-stamp':
        parsed.archiveStamp = argv[++index] || null;
        break;
      default:
        throw new Error(`unsupported proof-report argument: ${token}`);
    }
  }

  return parsed;
}

export function collectReport(options = {}) {
  const checkedAt = options.checkedAt || new Date().toISOString();
  const environment = {
    container: detectContainerEnvironment(),
    node: process.version,
    npm: detectCommandVersion('npm', ['--version']),
    cargo: detectCommandVersion('cargo', ['--version']),
    rustc: detectCommandVersion('rustc', ['--version'])
  };

  const version =
    extractTomlVersion(readText('Cargo.toml')) ||
    readJson('package.json').version ||
    '0.1.0';

  let sourceProof;
  let releaseProof;

  if (options.noRun) {
    sourceProof = {
      status: 'not-run',
      checks: [],
      reason: 'proof commands were skipped via --no-run'
    };
    releaseProof = {
      status: 'not-run',
      checks: [],
      reason: 'proof commands were skipped via --no-run'
    };
  } else if (!environment.npm) {
    sourceProof = {
      status: 'blocked',
      checks: [],
      reason: 'npm is not installed in this environment'
    };
    releaseProof = {
      status: 'blocked',
      checks: [],
      reason: 'npm is not installed in this environment'
    };
  } else {
    const source = runCheckedCommand('npm', ['test'], 'npm test');
    if (!source.ok) {
      sourceProof = {
        status: 'fail',
        checks: [],
        reason: source.error || summarizeFailure(source)
      };
      releaseProof = {
        status: 'blocked',
        checks: [],
        reason: 'source proof failed; release proof was not attempted'
      };
    } else {
      sourceProof = {
        status: 'pass',
        checks: [source.label],
        durationMs: source.durationMs
      };

      const releaseChecks = [source.label];
      const pack = runCheckedCommand('npm', ['run', 'pack:npm:dry-run'], 'npm run pack:npm:dry-run');
      if (!pack.ok) {
        releaseProof = {
          status: 'fail',
          checks: releaseChecks,
          reason: pack.error || summarizeFailure(pack)
        };
      } else {
        releaseChecks.push(pack.label);
        const archive = runArchiveBuilder(options.archiveOutputDir || DEFAULT_ARCHIVE_OUTPUT_DIR, options.archiveStamp || null);
        if (!archive.ok || !archive.archive) {
          releaseProof = {
            status: 'fail',
            checks: releaseChecks,
            reason: archive.error || summarizeFailure(archive)
          };
        } else {
          releaseChecks.push(archive.label);
          releaseProof = {
            status: 'partial',
            checks: releaseChecks,
            archive: {
              name: archive.archive.archiveName,
              path: normalizePathForReport(archive.archive.archivePath),
              stamp: archive.archive.stamp
            },
            missing: [
              'GitHub Release artifact publication',
              'published npm provenance proof',
              'real-host runtime validation before public release claims'
            ]
          };
        }
      }
    }
  }

  const buildProof = !environment.cargo || !environment.rustc
    ? {
        status: 'blocked',
        checks: [],
        reason: 'cargo/rustc are not installed in this environment'
      }
    : {
        status: 'not-run',
        checks: [],
        reason: 'Rust host proof was not executed by this report script'
      };

  const runtimeProof = environment.container
    ? {
        status: 'blocked',
        checks: [],
        reason: 'no supported real-host runtime proof was executed in this container'
      }
    : {
        status: 'not-run',
        checks: [],
        reason: 'runtime proof was not executed by this report script'
      };

  return {
    version,
    checkedAt,
    environment,
    sourceProof,
    buildProof,
    runtimeProof,
    releaseProof
  };
}

export function writeReport(report, outputPath = DEFAULT_OUTPUT_PATH) {
  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  fs.writeFileSync(outputPath, `${JSON.stringify(report, null, 2)}\n`, 'utf8');
  return outputPath;
}

function isCliInvocation() {
  const entry = process.argv[1];
  if (!entry) {
    return false;
  }
  return pathToFileURL(path.resolve(entry)).href === import.meta.url;
}

function main() {
  try {
    const parsed = parseArgs(process.argv.slice(2));
    const report = collectReport(parsed);
    if (parsed.write) {
      writeReport(report, parsed.outputPath);
      const archiveInfo = report.releaseProof?.archive;
      if (!parsed.noRun && archiveInfo?.stamp) {
        const resyncedArchive = runArchiveBuilder(parsed.archiveOutputDir, archiveInfo.stamp);
        if (!resyncedArchive.ok) {
          throw new Error(resyncedArchive.error || summarizeFailure(resyncedArchive));
        }
      }
    }
    if (parsed.json) {
      process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
      return;
    }
    process.stdout.write(`${parsed.write ? parsed.outputPath : 'proof report generated'}\n`);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exit(1);
  }
}

if (isCliInvocation()) {
  main();
}
