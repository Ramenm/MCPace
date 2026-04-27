#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import {
  RELEASE_TARGETS_PATH,
  assertReleaseTargetsManifest,
  enabledReleaseTargets,
  githubMatrixInclude,
  plannedReleaseTargets,
  releaseTargetsManifest
} from './lib/release-targets.mjs';
import { repoRoot } from './lib/project-metadata.mjs';

function parseArgs(argv) {
  const parsed = { json: false, workflows: [path.join('.github', 'workflows', 'release.yml'), path.join('.github', 'workflows', 'release-dry-run.yml')] };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--json': parsed.json = true; break;
      case '--workflow': parsed.workflows = [argv[++index] || parsed.workflows[0]]; break;
      default: throw new Error(`unsupported verify-release-targets argument: ${token}`);
    }
  }
  return parsed;
}

function workflowUsesGeneratedNativeMatrix(text, relativePath) {
  if (!text.includes('node scripts/github-release-matrix.mjs --github-output "$GITHUB_OUTPUT"')) return false;
  if (!/outputs:\s*\n\s+native_matrix:\s*\$\{\{\s*steps\.release-matrices\.outputs\.native_matrix\s*\}\}/m.test(text)) return false;

  const expectedOutput = relativePath.endsWith('release.yml')
    ? /matrix:\s*\$\{\{\s*fromJson\(needs\.source-release\.outputs\.native_matrix\)\s*\}\}/
    : /matrix:\s*\$\{\{\s*fromJson\(needs\.source-and-contracts\.outputs\.native_matrix\)\s*\}\}/;
  return expectedOutput.test(text);
}

function validateWorkflow(relativePath, targets) {
  const workflowPath = path.join(repoRoot, relativePath);
  const issues = [];
  if (!fs.existsSync(workflowPath)) return { path: relativePath, status: 'fail', mode: 'missing', issues: [`workflow does not exist: ${relativePath}`] };
  const text = fs.readFileSync(workflowPath, 'utf8');

  if (workflowUsesGeneratedNativeMatrix(text, relativePath)) {
    return { path: relativePath, status: 'pass', mode: 'generated-native-matrix', issues };
  }

  for (const target of targets) {
    for (const needle of [target.key, target.runner, target.triple, target.binaryName, target.packageName]) {
      if (!text.includes(needle)) issues.push(`${relativePath} matrix is missing ${needle}`);
    }
  }
  return { path: relativePath, status: issues.length === 0 ? 'pass' : 'fail', mode: 'inline-native-matrix', issues };
}

export function verifyReleaseTargets(options = {}) {
  const manifest = releaseTargetsManifest();
  const targets = enabledReleaseTargets(manifest);
  const planned = plannedReleaseTargets(manifest);
  const manifestIssues = assertReleaseTargetsManifest(manifest);
  const workflowPaths = options.workflows || [options.workflow || path.join('.github', 'workflows', 'release.yml')];
  const workflows = workflowPaths.map((workflowPath) => validateWorkflow(workflowPath, targets));
  const matrix = githubMatrixInclude(manifest);
  const matrixIssues = [];
  if (matrix.length !== targets.length) matrixIssues.push('generated native matrix size must match enabled release target count');
  for (const target of targets) {
    const entry = matrix.find((candidate) => candidate.target_key === target.key);
    if (!entry) {
      matrixIssues.push(`generated native matrix is missing ${target.key}`);
      continue;
    }
    if (entry.os !== target.runner) matrixIssues.push(`generated native matrix ${target.key} os must be ${target.runner}`);
    if (entry.rust_target !== target.triple) matrixIssues.push(`generated native matrix ${target.key} rust_target must be ${target.triple}`);
    if (entry.package_name !== target.packageName) matrixIssues.push(`generated native matrix ${target.key} package_name must be ${target.packageName}`);
    if (entry.binary_name !== target.binaryName) matrixIssues.push(`generated native matrix ${target.key} binary_name must be ${target.binaryName}`);
  }
  const issues = [...manifestIssues, ...matrixIssues, ...workflows.flatMap((workflow) => workflow.issues)];
  return {
    status: issues.length === 0 ? 'pass' : 'fail',
    manifestPath: path.relative(repoRoot, RELEASE_TARGETS_PATH).split(path.sep).join('/'),
    targetCount: targets.length,
    plannedTargetCount: planned.length,
    targets: targets.map((target) => ({ key: target.key, rustTarget: target.triple, runner: target.runner, npmPackage: target.packageName, binaryName: target.binaryName })),
    nativeMatrix: matrix.map((entry) => ({ targetKey: entry.target_key, rustTarget: entry.rust_target, runner: entry.os, npmPackage: entry.package_name, binaryName: entry.binary_name })),
    workflows,
    workflow: workflows[0] || null,
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
    const report = verifyReleaseTargets(parsed);
    if (parsed.json) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
    else if (report.status === 'pass') process.stdout.write(`release targets verified: ${report.targetCount} supported, ${report.plannedTargetCount} planned\n`);
    else process.stderr.write(`${report.issues.join('\n')}\n`);
    if (report.status !== 'pass') process.exit(1);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exit(1);
  }
}

if (isCliInvocation()) main();
