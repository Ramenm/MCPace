#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
import { buildReport as buildToolingReport } from './tooling-readiness.mjs';
import { deriveProjectName, deriveProjectVersion, repoRoot } from './lib/project-metadata.mjs';

function parseArgs(argv) {
  const out = { json: false, write: null, markdown: null, strict: false, timeoutMs: 7500, help: false };
  for (let i = 0; i < argv.length; i += 1) {
    const a = argv[i];
    if (a === '--json') out.json = true;
    else if (a === '--write') out.write = argv[++i] || null;
    else if (a === '--markdown' || a === '--write-md') out.markdown = argv[++i] || null;
    else if (a === '--strict') out.strict = true;
    else if (a === '--timeout-ms') out.timeoutMs = Number(argv[++i] || 0);
    else if (a === '-h' || a === '--help') out.help = true;
    else throw new Error(`unsupported toolbox-doctor argument: ${a}`);
  }
  if (!Number.isSafeInteger(out.timeoutMs) || out.timeoutMs <= 0) out.timeoutMs = 7500;
  return out;
}

function recommendedCommands() {
  return [
    { id: 'fast-loop', command: 'npm run verify:local:smoke', use: 'fastest local sanity loop while editing' },
    { id: 'source-proof', command: 'npm run verify:local:source', use: 'source snapshot proof before pushing or sharing a ZIP' },
    { id: 'full-proof', command: 'npm run verify:local:full', use: 'runtime/native proof on a host with Cargo dependency access' },
    { id: 'publish-decision', command: 'npm run verify:publish-decision', use: 'single yes/no decision for public source snapshot vs native npm publication' },
  ];
}

function buildReport(opts) {
  const tooling = buildToolingReport({ timeoutMs: opts.timeoutMs, strict: opts.strict });
  const requiredBlocked = tooling.tools.filter((tool) => tool.requirement === 'required' && tool.status === 'blocked');
  const recommendedMissing = tooling.tools.filter((tool) => tool.requirement !== 'required' && tool.status !== 'pass');
  return {
    schema: 'mcpace.toolboxDoctor.v1',
    generatedAt: new Date().toISOString(),
    project: { name: deriveProjectName(), version: deriveProjectVersion() },
    status: requiredBlocked.length ? 'blocked' : recommendedMissing.length ? 'ready-with-warnings' : 'ready',
    localOnly: true,
    githubPaidPlanRequired: false,
    host: { platform: process.platform, arch: process.arch, node: process.version, cwd: repoRoot },
    requiredBlocked: requiredBlocked.map((tool) => tool.id),
    recommendedMissing: recommendedMissing.map((tool) => tool.id),
    commands: recommendedCommands(),
    tooling,
    nextActions: [
      ...requiredBlocked.map((tool) => tool.installHint).filter(Boolean),
      ...recommendedMissing.map((tool) => tool.installHint).filter(Boolean),
    ],
  };
}

function renderMarkdown(report) {
  return [
    '# MCPace toolbox doctor', '',
    `Project: \`${report.project.name}\` v\`${report.project.version}\``,
    `Status: \`${report.status}\``,
    `GitHub paid plan required: \`${report.githubPaidPlanRequired ? 'yes' : 'no'}\``,
    '', '## Recommended local commands', '',
    '| command | use |', '|---|---|',
    ...report.commands.map((row) => `| \`${row.command}\` | ${row.use} |`),
    '', '## Tooling summary', '',
    '| tool | requirement | status | evidence |', '|---|---:|---:|---|',
    ...report.tooling.tools.map((tool) => `| ${tool.id} | ${tool.requirement} | ${tool.status} | ${String(tool.evidence || '').replace(/\|/g, '\\|')} |`),
    report.nextActions.length ? '\n## Next actions\n' : '',
    ...report.nextActions.map((action) => `- ${action}`), '',
  ].filter((line) => line !== '').join('\n');
}

function writeArtifacts(report, opts) {
  for (const [file, contents] of [[opts.write, `${JSON.stringify(report, null, 2)}\n`], [opts.markdown, renderMarkdown(report)]]) {
    if (!file) continue;
    const target = path.resolve(repoRoot, file);
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, contents, 'utf8');
  }
}

function main() {
  const opts = parseArgs(process.argv.slice(2));
  if (opts.help) {
    process.stdout.write('Usage: node scripts/toolbox-doctor.mjs [--json] [--write <path>] [--markdown <path>] [--strict]\n');
    return;
  }
  const report = buildReport(opts);
  writeArtifacts(report, opts);
  process.stdout.write(opts.json ? `${JSON.stringify(report, null, 2)}\n` : renderMarkdown(report));
  if (opts.strict && report.status === 'blocked') process.exitCode = 1;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { main(); } catch (err) { process.stderr.write(`${err.message || err}\n`); process.exitCode = 1; }
}

export { buildReport, renderMarkdown };
