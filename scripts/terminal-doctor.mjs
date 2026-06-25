#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import {
  collectTerminalDiagnostics,
  formatTerminalDiagnostics
} from '../packages/npm/cli/lib/terminal-diagnostics.js';
import { repoRoot } from './lib/project-metadata.mjs';

const args = new Set(process.argv.slice(2));
const jsonOutput = args.has('--json');

function pathStatus(filePath) {
  try {
    const stat = fs.statSync(filePath);
    return {
      path: filePath,
      exists: true,
      file: stat.isFile(),
      executable: process.platform === 'win32' ? stat.isFile() : Boolean(stat.mode & 0o111),
    };
  } catch {
    return { path: filePath, exists: false, file: false, executable: false };
  }
}

function localBinPath(command) {
  const suffix = process.platform === 'win32' ? `${command}.cmd` : command;
  return path.join(repoRoot, 'node_modules', '.bin', suffix);
}

function buildReport() {
  const npmDiagnostics = collectTerminalDiagnostics({ invokedAs: 'doctor:terminal' });
  const report = {
    schema: 'mcpace.terminalDoctor.v1',
    status: 'pass',
    npmDiagnostics,
    localBins: {
      mcpace: pathStatus(localBinPath('mcpace')),
      packageShim: pathStatus(path.join(repoRoot, 'packages', 'npm', 'cli', 'bin', 'mcpace.js')),
    },
    recommendations: [...npmDiagnostics.hints],
  };

  const hasPathCommand = Object.values(npmDiagnostics.path.commands).some((matches) => matches.length > 0);
  if (!hasPathCommand && !report.localBins.mcpace.exists) {
    report.status = 'fail';
    report.recommendations.unshift('No MCPace command was found on PATH and no local node_modules/.bin/mcpace exists. Run npm ci, npm link, npm install -g, or use npm exec from an installed project.');
  } else if (npmDiagnostics.resolution.status !== 'resolved' || !npmDiagnostics.node.supported || !hasPathCommand) {
    report.status = 'warn';
  }

  if (report.localBins.mcpace.exists && !hasPathCommand) {
    report.recommendations.push('A local node_modules/.bin/mcpace exists but is not on your interactive PATH. Use `npm exec -- mcpace ...`, `npm run`, or call the local bin path directly.');
  }
  if (report.recommendations.length === 0) {
    report.recommendations.push('Terminal command lookup looks healthy. If failures are intermittent, capture `npm run doctor:terminal -- --json` from a failing shell and compare PATH, Node version, and native binary resolution.');
  }
  return report;
}

const report = buildReport();
if (jsonOutput) {
  process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
} else {
  process.stdout.write(`MCPace terminal doctor: ${report.status}\n`);
  process.stdout.write(formatTerminalDiagnostics(report.npmDiagnostics));
  process.stdout.write(`local mcpace: ${report.localBins.mcpace.exists ? report.localBins.mcpace.path : 'not found'}\n`);
  for (const tip of report.recommendations) {
    process.stdout.write(`- ${tip}\n`);
  }
}

if (report.status === 'fail') process.exitCode = 1;
