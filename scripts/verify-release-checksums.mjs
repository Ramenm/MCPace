#!/usr/bin/env node
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { repoRoot } from './lib/project-metadata.mjs';

const DEFAULT_ARTIFACT_DIR = path.join(repoRoot, 'dist', 'npm');
const DEFAULT_CHECKSUM_FILENAME = 'SHA256SUMS.txt';
const DEFAULT_REQUIRED_EXTENSION = '.tgz';

function parseArgs(argv) {
  const parsed = {
    json: false,
    artifactDir: DEFAULT_ARTIFACT_DIR,
    checksumPath: null,
    requiredExtension: DEFAULT_REQUIRED_EXTENSION
  };

  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--json':
        parsed.json = true;
        break;
      case '--artifact-dir':
        parsed.artifactDir = path.resolve(argv[++index] || '');
        break;
      case '--checksum-path':
        parsed.checksumPath = path.resolve(argv[++index] || '');
        break;
      case '--required-extension':
        parsed.requiredExtension = argv[++index] || '';
        break;
      default:
        throw new Error(`unsupported verify-release-checksums argument: ${token}`);
    }
  }

  return parsed;
}

function sha256ForFile(filePath) {
  const hash = crypto.createHash('sha256');
  hash.update(fs.readFileSync(filePath));
  return hash.digest('hex');
}

function normalizeEntryName(value) {
  return String(value || '').trim().replace(/^\*?/, '').replace(/\\/g, '/').replace(/^\.\//, '');
}

function parseChecksumManifest(text) {
  const entries = [];
  const issues = [];
  const lines = text.split(/\r?\n/);

  lines.forEach((line, index) => {
    const trimmed = line.trim();
    if (!trimmed) return;
    const match = trimmed.match(/^([a-fA-F0-9]{64})\s+(.+)$/);
    if (!match) {
      issues.push(`invalid checksum line ${index + 1}: ${line}`);
      return;
    }
    entries.push({ sha256: match[1].toLowerCase(), name: normalizeEntryName(match[2]), line: index + 1 });
  });

  return { entries, issues };
}

function listArtifacts(artifactDir, requiredExtension) {
  if (!fs.existsSync(artifactDir)) return [];
  return fs.readdirSync(artifactDir)
    .filter((name) => name.endsWith(requiredExtension))
    .map((name) => path.join(artifactDir, name))
    .filter((filePath) => fs.statSync(filePath).isFile())
    .sort((left, right) => left.localeCompare(right));
}

function resolveArtifactPath(entryName, artifactDir) {
  const candidates = [
    path.join(artifactDir, entryName),
    path.join(artifactDir, path.basename(entryName))
  ];
  const seen = new Set();
  for (const candidate of candidates) {
    const absolute = path.resolve(candidate);
    if (seen.has(absolute)) continue;
    seen.add(absolute);
    if (fs.existsSync(absolute) && fs.statSync(absolute).isFile()) return absolute;
  }
  return null;
}

export function verifyReleaseChecksums(options = {}) {
  const artifactDir = path.resolve(options.artifactDir || DEFAULT_ARTIFACT_DIR);
  const checksumPath = path.resolve(options.checksumPath || path.join(artifactDir, DEFAULT_CHECKSUM_FILENAME));
  const requiredExtension = options.requiredExtension || DEFAULT_REQUIRED_EXTENSION;
  const issues = [];
  const verified = [];

  if (!fs.existsSync(checksumPath)) {
    return { status: 'fail', artifactDir, checksumPath, requiredExtension, verified, issues: [`missing checksum manifest: ${checksumPath}`] };
  }

  const parsed = parseChecksumManifest(fs.readFileSync(checksumPath, 'utf8'));
  issues.push(...parsed.issues);
  const relevantEntries = parsed.entries.filter((entry) => entry.name.endsWith(requiredExtension));
  if (relevantEntries.length === 0) {
    issues.push(`checksum manifest contains no ${requiredExtension} entries`);
  }

  const artifactPaths = listArtifacts(artifactDir, requiredExtension);
  const coveredBasenames = new Set(relevantEntries.map((entry) => path.basename(entry.name)));
  for (const artifactPath of artifactPaths) {
    const basename = path.basename(artifactPath);
    if (!coveredBasenames.has(basename)) issues.push(`artifact is not covered by checksum manifest: ${basename}`);
  }

  for (const entry of relevantEntries) {
    const artifactPath = resolveArtifactPath(entry.name, artifactDir);
    if (!artifactPath) {
      issues.push(`checksum entry has no downloaded artifact: ${entry.name}`);
      continue;
    }
    const actual = sha256ForFile(artifactPath);
    const status = actual === entry.sha256 ? 'pass' : 'fail';
    verified.push({ name: entry.name, artifactPath, expectedSha256: entry.sha256, actualSha256: actual, status });
    if (status !== 'pass') issues.push(`checksum mismatch for ${path.basename(artifactPath)}`);
  }

  return {
    status: issues.length === 0 ? 'pass' : 'fail',
    artifactDir,
    checksumPath,
    requiredExtension,
    checkedCount: verified.length,
    verified,
    issues
  };
}

function isCliInvocation() {
  const entry = process.argv[1];
  return entry ? pathToFileURL(path.resolve(entry)).href === import.meta.url : false;
}

function main() {
  try {
    const parsed = parseArgs(process.argv.slice(2));
    const report = verifyReleaseChecksums(parsed);
    if (parsed.json) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
    else if (report.status === 'pass') process.stdout.write(`verified ${report.checkedCount} ${report.requiredExtension} checksums\n`);
    else process.stderr.write(`${report.issues.join('\n')}\n`);
    if (report.status !== 'pass') process.exit(1);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exit(1);
  }
}

if (isCliInvocation()) main();
