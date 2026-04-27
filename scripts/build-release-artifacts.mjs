#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { collectReport, writeReport } from './proof-report.mjs';
import { generateChecksums } from './generate-checksums.mjs';
import {
  deriveProjectName,
  deriveProjectVersion,
  repoRoot
} from './lib/project-metadata.mjs';

const DEFAULT_OUTPUT_DIR = path.join(repoRoot, 'dist');
const REPORT_FILENAME = 'verification-latest.json';
const CHECKSUMS_FILENAME = 'SHA256SUMS.txt';
const MANIFEST_FILENAME = 'release-artifacts.json';
const DEFAULT_REPO_REPORT_PATH = path.join(repoRoot, 'reports', REPORT_FILENAME);

function normalizePathForReport(filePath) {
  const relative = path.relative(repoRoot, filePath);
  return relative && !relative.startsWith('..') ? relative.split(path.sep).join('/') : filePath;
}

function parseExistingPath(token, value) {
  if (!value) {
    throw new Error(`${token} requires a path`);
  }
  return path.resolve(value);
}

export function parseArgs(argv) {
  const parsed = {
    json: false,
    outputDir: DEFAULT_OUTPUT_DIR,
    archiveStamp: process.env.MCPACE_ARCHIVE_TIMESTAMP || null,
    checkedAt: null,
    clean: true,
    existingReportPath: null,
    existingArchivePath: null,
    syncRepoReport: true
  };

  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--json':
        parsed.json = true;
        break;
      case '--output-dir':
        parsed.outputDir = path.resolve(argv[++index] || '');
        break;
      case '--archive-stamp':
        parsed.archiveStamp = argv[++index] || null;
        break;
      case '--checked-at':
        parsed.checkedAt = argv[++index] || null;
        break;
      case '--no-clean':
        parsed.clean = false;
        break;
      case '--existing-report-path':
        parsed.existingReportPath = parseExistingPath(token, argv[++index]);
        break;
      case '--existing-archive-path':
        parsed.existingArchivePath = parseExistingPath(token, argv[++index]);
        break;
      case '--no-sync-repo-report':
        parsed.syncRepoReport = false;
        break;
      default:
        throw new Error(`unsupported build-release-artifacts argument: ${token}`);
    }
  }

  if (Boolean(parsed.existingReportPath) !== Boolean(parsed.existingArchivePath)) {
    throw new Error('existing release inputs require both --existing-report-path and --existing-archive-path');
  }

  return parsed;
}

function cleanOutputDir(outputDir) {
  if (!fs.existsSync(outputDir)) {
    return;
  }
  for (const entry of fs.readdirSync(outputDir, { withFileTypes: true })) {
    fs.rmSync(path.join(outputDir, entry.name), { recursive: true, force: true });
  }
}

function ensureDir(outputDir) {
  fs.mkdirSync(outputDir, { recursive: true });
}

function readReportFromPath(reportPath) {
  const raw = fs.readFileSync(reportPath, 'utf8');
  return JSON.parse(raw);
}

function copyArtifact(sourcePath, destinationPath) {
  if (!fs.existsSync(sourcePath)) {
    throw new Error(`release artifact does not exist: ${sourcePath}`);
  }
  const stat = fs.statSync(sourcePath);
  if (!stat.isFile()) {
    throw new Error(`release artifact is not a file: ${sourcePath}`);
  }
  fs.mkdirSync(path.dirname(destinationPath), { recursive: true });
  fs.copyFileSync(sourcePath, destinationPath);
  return destinationPath;
}

function resolveReportAndArchive(options, outputDir) {
  const reportPath = path.join(outputDir, REPORT_FILENAME);

  if (options.existingReportPath && options.existingArchivePath) {
    const report = readReportFromPath(options.existingReportPath);
    const expectedVersion = deriveProjectVersion();
    if (report.version !== expectedVersion) {
      throw new Error(
        `existing verification report version drift: expected ${expectedVersion}, got ${report.version || 'missing version'}`
      );
    }

    const reportedArchiveName = report.releaseProof?.archive?.name || null;
    const archiveName = path.basename(options.existingArchivePath);
    if (!reportedArchiveName) {
      throw new Error('existing verification report is missing releaseProof.archive.name');
    }
    if (reportedArchiveName !== archiveName) {
      throw new Error(
        `existing release artifact mismatch: report expects ${reportedArchiveName}, got ${archiveName}`
      );
    }

    const bundledArchivePath = copyArtifact(options.existingArchivePath, path.join(outputDir, archiveName));
    writeReport(report, reportPath);
    return {
      report,
      reportPath,
      archivePath: bundledArchivePath,
      archiveName,
      source: 'existing-artifacts'
    };
  }

  const report = collectReport({
    checkedAt: options.checkedAt,
    archiveOutputDir: outputDir,
    archiveStamp: options.archiveStamp
  });
  const archiveName = report.releaseProof?.archive?.name || null;
  if (!archiveName) {
    const reason = report.releaseProof?.reason || 'release proof did not produce an archive';
    throw new Error(reason);
  }

  const archivePath = path.join(outputDir, archiveName);
  if (!fs.existsSync(archivePath)) {
    throw new Error(`release proof reported archive '${archiveName}' but it was not found in ${outputDir}`);
  }

  writeReport(report, reportPath);
  if (options.syncRepoReport !== false) {
    writeReport(report, DEFAULT_REPO_REPORT_PATH);
  }
  return {
    report,
    reportPath,
    archivePath,
    archiveName,
    source: 'fresh-proof-run',
    repoReportPath: options.syncRepoReport !== false ? DEFAULT_REPO_REPORT_PATH : null
  };
}

function enrichChecksums(entries, archivePath, reportPath) {
  const byName = new Map(entries.map((entry) => [entry.name, entry]));
  return {
    archive: byName.get(path.basename(archivePath)) || null,
    verificationReport: byName.get(path.basename(reportPath)) || null
  };
}

export function buildReleaseArtifacts(options = {}) {
  const outputDir = path.resolve(options.outputDir || DEFAULT_OUTPUT_DIR);
  if (options.clean !== false) {
    cleanOutputDir(outputDir);
  }
  ensureDir(outputDir);

  const resolved = resolveReportAndArchive(options, outputDir);
  const checksumsPath = path.join(outputDir, CHECKSUMS_FILENAME);
  const checksums = generateChecksums({
    outputDir,
    outputPath: checksumsPath,
    include: [resolved.archivePath, resolved.reportPath]
  });
  const checksummed = enrichChecksums(checksums.entries, resolved.archivePath, resolved.reportPath);

  const manifest = {
    projectName: deriveProjectName(),
    version: resolved.report.version || deriveProjectVersion(),
    builtAt: resolved.report.checkedAt || options.checkedAt || new Date().toISOString(),
    source: resolved.source,
    outputDir: normalizePathForReport(outputDir),
    archive: {
      name: path.basename(resolved.archivePath),
      path: normalizePathForReport(resolved.archivePath),
      sha256: checksummed.archive?.sha256 || null
    },
    verificationReport: {
      name: path.basename(resolved.reportPath),
      path: normalizePathForReport(resolved.reportPath),
      sha256: checksummed.verificationReport?.sha256 || null,
      sourceProofStatus: resolved.report.sourceProof?.status || 'unknown',
      buildProofStatus: resolved.report.buildProof?.status || 'unknown',
      runtimeProofStatus: resolved.report.runtimeProof?.status || 'unknown',
      releaseProofStatus: resolved.report.releaseProof?.status || 'unknown'
    },
    distribution: {
      currentTarget: resolved.report.distribution?.currentTarget || null,
      currentTargetPackagingMode: resolved.report.distribution?.currentTargetPackagingMode || null,
      vendoredBinaryTargets: resolved.report.distribution?.vendoredBinaryTargets || []
    },
    checksums: {
      name: CHECKSUMS_FILENAME,
      path: normalizePathForReport(checksumsPath),
      fileCount: checksums.fileCount,
      entries: checksums.entries
    }
  };

  const manifestPath = path.join(outputDir, MANIFEST_FILENAME);
  fs.writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`, 'utf8');

  return {
    outputDir: normalizePathForReport(outputDir),
    archive: manifest.archive,
    verificationReport: manifest.verificationReport,
    checksums: manifest.checksums,
    manifestPath: normalizePathForReport(manifestPath),
    source: manifest.source,
    distribution: manifest.distribution,
    releaseProofStatus: manifest.verificationReport.releaseProofStatus
  };
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
    const result = buildReleaseArtifacts(parsed);
    if (parsed.json) {
      process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
      return;
    }
    process.stdout.write(`${result.manifestPath}\n`);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exit(1);
  }
}

if (isCliInvocation()) {
  main();
}
