#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { githubMatrixInclude } from './lib/release-targets.mjs';

function parseArgs(argv) {
  const parsed = { json: false, githubOutput: process.env.GITHUB_OUTPUT || null };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--json':
        parsed.json = true;
        break;
      case '--github-output':
        parsed.githubOutput = argv[++index] || '';
        break;
      default:
        throw new Error(`unsupported github-release-matrix argument: ${token}`);
    }
  }
  return parsed;
}

function normalizeNativeEntry(entry) {
  return {
    target_key: entry.target_key,
    package_name: entry.package_name,
    os: entry.os || entry.runner,
    rust_target: entry.rust_target,
    binary_name: entry.binary_name
  };
}

function validateNativeMatrix(matrix) {
  const issues = [];
  const seenTargetKeys = new Set();
  const seenPackages = new Set();

  if (!Array.isArray(matrix.include) || matrix.include.length === 0) {
    issues.push('native release matrix must include at least one target');
    return issues;
  }

  for (const entry of matrix.include) {
    for (const field of ['target_key', 'package_name', 'os', 'rust_target', 'binary_name']) {
      if (!entry[field]) issues.push(`native release matrix entry is missing ${field}: ${JSON.stringify(entry)}`);
    }
    if (seenTargetKeys.has(entry.target_key)) issues.push(`duplicate native release target ${entry.target_key}`);
    seenTargetKeys.add(entry.target_key);
    if (seenPackages.has(entry.package_name)) issues.push(`duplicate native release package ${entry.package_name}`);
    seenPackages.add(entry.package_name);
    if (entry.target_key?.startsWith('win32-') && entry.binary_name !== 'mcpace.exe') {
      issues.push(`${entry.target_key} must use mcpace.exe`);
    }
    if (!entry.target_key?.startsWith('win32-') && entry.binary_name !== 'mcpace') {
      issues.push(`${entry.target_key} must use mcpace`);
    }
  }

  return issues;
}

export function nativeReleaseMatrix() {
  const matrix = { include: githubMatrixInclude().map(normalizeNativeEntry) };
  const issues = validateNativeMatrix(matrix);
  if (issues.length > 0) {
    throw new Error(issues.join('\n'));
  }
  return matrix;
}

function appendGithubOutput(outputPath, values) {
  if (!outputPath) return;
  const absolutePath = path.resolve(outputPath);
  const lines = Object.entries(values).map(([key, value]) => `${key}=${value}`);
  fs.appendFileSync(absolutePath, `${lines.join('\n')}\n`, 'utf8');
}

export function releaseMatricesReport() {
  const nativeMatrix = nativeReleaseMatrix();
  return {
    status: 'pass',
    nativeMatrix,
    nativeMatrixJson: JSON.stringify(nativeMatrix)
  };
}

function isCliInvocation() {
  const entry = process.argv[1];
  return entry ? pathToFileURL(path.resolve(entry)).href === import.meta.url : false;
}

function main() {
  try {
    const parsed = parseArgs(process.argv.slice(2));
    const report = releaseMatricesReport();
    appendGithubOutput(parsed.githubOutput, { native_matrix: report.nativeMatrixJson });
    if (parsed.json) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
    else process.stdout.write(`generated native release matrix for ${report.nativeMatrix.include.length} targets\n`);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exit(1);
  }
}

if (isCliInvocation()) main();
